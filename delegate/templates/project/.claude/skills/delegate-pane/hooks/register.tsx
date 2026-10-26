import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register, ThemeKey } from 'claude-code'

import type { PaneState, Task, View } from '../types'

// Shows `delegate status --json` in a pane beside the chat, read again every few seconds.
const PANE = 'delegate'
const EVERY_MS = 3000
const LIVE = new Set(['questions', 'implement', 'checks', 'review'])
const view = atom({ plugin: 'delegate-pane', key: 'view' } as const, {
  tasks: [],
  error: null,
  readAt: 0,
} as View)

const pane = atom({ plugin: 'delegate-pane', key: 'pane' } as const, {
  running: [],
  autoOpened: false,
} as PaneState)

async function syncPane($: EngineInterface, tasks: Task[]): Promise<void> {
  const previous = await read($, pane)
  const running = tasks.filter(t => LIVE.has(t.phase)).map(t => t.id)
  // Moving between running phases must not undo a person's close.
  const started = running.some(id => !previous.running.includes(id))
  await update($, pane, state => ({ ...state, running }))
  if (started && !(await $.ui.panes()).some(p => p.id === PANE)) {
    await $.ui.open({ id: PANE, title: 'Delegate tasks' })
    await update($, pane, state => ({ ...state, autoOpened: true }))
  } else if (!tasks.some(t => t.phase !== 'not_started') && previous.autoOpened) {
    await $.ui.close({ id: PANE })
    await update($, pane, state => ({ ...state, autoOpened: false }))
  }
}

async function refresh($: EngineInterface): Promise<View> {
  let next: View
  try {
    const out = await $.process.run(['delegate', 'status', '--json'], { timeoutMs: 20000 })
    if (out.exitCode !== 0) {
      const why = out.stderr.trim().split('\n').pop() ?? ''
      next = { tasks: [], error: why || 'delegate status failed', readAt: Date.now() }
    } else {
      next = { tasks: (JSON.parse(out.stdout).tasks ?? []) as Task[], error: null, readAt: Date.now() }
    }
  } catch (err) {
    next = { tasks: [], error: `delegate isn't available here (${String(err)})`, readAt: Date.now() }
  }
  await update($, view, () => next)
  // A failed read says nothing about whether the workers stopped.
  if (!next.error) await syncPane($, next.tasks)
  return next
}

function ago(iso: string | null, now: number): string {
  if (!iso) return '-'
  const s = Math.max(0, Math.round((now - Date.parse(iso)) / 1000))
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m${s % 60}s` : `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`
}

function failed(t: Task): boolean {
  return t.phase === 'interrupted' || (t.phase === 'done' && t.outcome === 'fail')
}

function group(t: Task): number {
  return t.stalled || failed(t) ? 0 : t.phase === 'done' ? 1 : LIVE.has(t.phase) ? 2 : 3
}

function label(t: Task, now: number): { text: string; color: ThemeKey } {
  if (t.stalled) return { text: `⚠ STALLED · quiet ${ago(t.last_activity_at, now)}`, color: 'warning' }
  if (t.phase === 'interrupted') return { text: '✗ interrupted', color: 'error' }
  if (t.phase === 'done' && t.outcome === 'fail') return { text: '✗ failed', color: 'error' }
  if (t.phase === 'done' && t.outcome === 'pass')
    return {
      text: t.ready_to_merge ? '✓ ready to merge' : `✓ passed · ${t.merge_blockers?.[0] || 'not ready to merge'}`,
      color: 'success',
    }
  if (t.phase === 'done') return { text: `done, ${t.outcome ?? '-'}`, color: 'inactive' }
  if (t.phase === 'not_started') return { text: 'not started', color: 'inactive' }
  return { text: `● ${t.phase === 'implement' ? 'implementing' : t.phase}`, color: 'suggestion' }
}

const graphemes = new Intl.Segmenter(undefined, { granularity: 'grapheme' })

function cellWidth(text: string): number {
  if (/^[\p{Mark}\p{Control}\p{Format}]+$/u.test(text)) return 0
  return /[\p{Extended_Pictographic}\p{Regional_Indicator}\u1100-\u115f\u2329\u232a\u2e80-\ua4cf\uac00-\ud7a3\uf900-\ufaff\ufe10-\ufe19\ufe30-\ufe6f\uff01-\uff60\uffe0-\uffe6\u{20000}-\u{3fffd}]/u.test(text) ? 2 : 1
}

function clip(text: string, columns: number): string {
  const cells = Array.from(graphemes.segment(text.replace(/[\r\n\t]/g, ' ')), part => part.segment)
  const width = Math.max(0, columns)
  if (cells.reduce((n, c) => n + cellWidth(c), 0) <= width) return cells.join('')
  if (width === 0) return ''
  let result = ''
  let used = 0
  for (const c of cells) {
    if (used + cellWidth(c) > width - 1) break
    result += c
    used += cellWidth(c)
  }
  return `${result}…`
}

type SummaryPart = { text: string; color?: ThemeKey }

function summary(tasks: Task[]): SummaryPart[] {
  const groups = [
    { text: 'stalled', icon: '⚠', color: 'warning' },
    { text: 'interrupted', icon: '✗', color: 'error' },
    { text: 'failed', icon: '✗', color: 'error' },
    { text: 'ready to merge', icon: '✓', color: 'success' },
    { text: 'passed', icon: '✓', color: 'success' },
    { text: 'running', icon: '●', color: 'suggestion' },
    { text: 'not started', icon: '', color: 'inactive' },
  ] as const
  const counts = groups.map(() => 0)
  for (const t of tasks) {
    const index = t.stalled ? 0 : t.phase === 'interrupted' ? 1
      : failed(t) ? 2 : t.phase === 'done' && t.outcome === 'pass' ? (t.ready_to_merge ? 3 : 4)
      : LIVE.has(t.phase) ? 5 : t.phase === 'not_started' ? 6 : -1
    if (index >= 0) counts[index] += 1
  }
  const parts: SummaryPart[] = [{ text: 'delegate  ' }]
  groups.forEach((g, i) => {
    if (!counts[i]) return
    if (parts.length > 1) parts.push({ text: ' · ' })
    parts.push({ text: `${g.icon ? `${g.icon} ` : ''}${counts[i]} ${g.text}`, color: g.color })
  })
  return parts
}

function clipSummary(parts: SummaryPart[], columns: number): SummaryPart[] {
  if (columns <= 0) return []
  const width = (text: string) => Array.from(graphemes.segment(text), p => cellWidth(p.segment)).reduce((a, b) => a + b, 0)
  const total = parts.reduce((n, p) => n + width(p.text), 0)
  const tail = ' · Open tasks ›'
  if (total + width(tail) <= columns) return [...parts, { text: tail }]
  if (total <= columns) return parts
  let remaining = Math.max(0, columns - 1)
  const clipped: SummaryPart[] = []
  for (const p of parts) {
    let text = ''
    for (const { segment } of graphemes.segment(p.text)) {
      const cells = cellWidth(segment)
      if (cells > remaining) return [...clipped, { ...p, text: `${text}…` }]
      text += segment
      remaining -= cells
    }
    clipped.push({ ...p, text })
  }
  return columns > 0 ? clipped : []
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const ran = await next(e)
    await $.command.register({ name: 'delegate-pane', description: 'Show delegate tasks in a pane' })
    await refresh($)
    $.clock.every(EVERY_MS, () => refresh($))
    return ran
  })

  on('command.run', { command: 'delegate-pane' }, async $ => {
    await refresh($)
    await $.ui.open({ id: PANE, title: 'Delegate tasks' })
    await update($, pane, state => ({ ...state, autoOpened: false }))
    return { text: 'Delegate tasks pane opened.' }
  })

  // The pane registry isn't an atom: redraw the band after any open or close.
  on('ui.open', { id: PANE }, async ($, e, next) => {
    const result = await next(e)
    $.ui.invalidate('ui.render')
    return result
  })

  on('ui.close', { id: PANE }, async ($, e, next) => {
    const result = await next(e)
    $.ui.invalidate('ui.render')
    return result
  })

  on('ui.close', { id: PANE }, async ($, e, next) => {
    const closed = await next(e)
    if (e.origin.kind === 'person')
      await update($, pane, state => ({ ...state, autoOpened: false }))
    return closed
  })

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    const v = await read($, view)
    await read($, pane)
    if (e.props.hasSurvey || (await $.ui.panes()).some(p => p.id === PANE)
      || (!v.error && !v.tasks.some(t => t.phase !== 'not_started')))
      return next(e)
    const { Button, Text } = $.ui.resolve(e)
    const parts = clipSummary(v.error ? [{ text: 'delegate: status unavailable' }] : summary(v.tasks), e.props.bodyColumns)
    return (
      <Button key="delegate-summary" label={parts.length ? undefined : 'Delegate tasks'} plain onPress={async () => {
        await $.ui.open({ id: PANE, title: 'Delegate tasks' })
        await update($, pane, state => ({ ...state, autoOpened: false }))
      }}>
        {parts.length ? parts.map(p => <Text color={p.color} dimColor={!p.color}>{p.text}</Text>) : <Text>{''}</Text>}
      </Button>
    )
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text } = $.ui.resolve(e)
    const v = await read($, view)
    const now = v.readAt || Date.now()
    if (v.error) return <Box flexDirection="column"><Text dimColor>{v.error}</Text></Box>
    if (v.tasks.length === 0)
      return (
        <Box flexDirection="column">
          <Text>No delegate tasks yet.</Text>
          <Text dimColor>Start one with: delegate new t001</Text>
        </Box>
      )
    return (
      <Box flexDirection="column">
        {[...v.tasks].sort((a, b) => group(a) - group(b) || a.id.localeCompare(b.id)).map(t => {
          const l = label(t, now)
          const live = LIVE.has(t.phase)
          const n = t.changed_files.length
          const files = `${n} ${n === 1 ? 'file' : 'files'}`
          const bar: ThemeKey | undefined = t.stalled ? 'warning' : failed(t) ? 'error' : undefined
          const lines = [
            <Text color={l.color} bold={t.stalled}>{l.text}</Text>,
            <Text><Text bold>{t.id}</Text>{t.worker ? <Text dimColor>  {t.worker}</Text> : null}</Text>,
          ]
          if (t.phase !== 'not_started')
            lines.push(<Text dimColor>{live
              ? `running ${ago(t.started_at, now)} · quiet ${ago(t.last_activity_at, now)} · ${n ? files : 'no files yet'}`
              : `done · ${files}`}</Text>)
          if (live)
            for (const s of t.steps.slice(-3))
              lines.push(<Text dimColor wrap="truncate-end">{clip(`  ${s.kind} ${s.target ?? ''} ${s.result ?? ''}`, e.props.bodyColumns - 2)}</Text>)
          if ((t.stalled || failed(t) || t.ready_to_merge) && (t.next || t.stalled))
            lines.push(<Text>→ {t.next || `delegate status ${t.id}`}</Text>)
          return (
            <Box key={t.id} flexDirection="column" marginBottom={1}>
              {lines.map(line => (
                <Box flexDirection="row">
                  <Box width={1} flexShrink={0} marginRight={1}><Text color={bar}>{bar ? '┃' : ' '}</Text></Box>
                  <Box minWidth={0} flexShrink={1}>{line}</Box>
                </Box>
              ))}
            </Box>
          )
        })}
      </Box>
    )
  })
}
