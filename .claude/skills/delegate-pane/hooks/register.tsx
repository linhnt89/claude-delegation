import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register, ThemeKey } from 'claude-code'

import type { Task, View } from '../types'

// Shows `delegate status --json` in a pane beside the chat, read again every few seconds.
const PANE = 'delegate'
const EVERY_MS = 3000
const LIVE = new Set(['questions', 'implement', 'checks', 'review'])
const view = atom({ plugin: 'delegate-pane', key: 'view' } as const, {
  tasks: [],
  error: null,
  readAt: 0,
} as View)

const reading = new Set<string | null>()

async function refresh($: EngineInterface): Promise<void> {
  const id = await sessionId($)
  // Skip busy ticks, rather than queueing reads behind a slow delegate.
  if (reading.has(id)) return
  reading.add(id)
  try {
    await readStatus($)
  } finally {
    reading.delete(id)
  }
}

async function readStatus($: EngineInterface): Promise<void> {
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

async function sessionId($: EngineInterface): Promise<string | null> {
  try {
    return (await $.session.id()) || null
  } catch {
    // Older engines may not expose the viewer's session id.
    return null
  }
}

type SessionGroup = { session: string | null; name: string; mine: boolean; tasks: Task[] }

function sessions(tasks: Task[], id: string | null): SessionGroup[] {
  if (id === null)
    return [{ session: null, name: tasks[0]?.session_name ?? 'by hand', mine: true, tasks }]
  const groups = new Map<string | null, SessionGroup>()
  for (const t of tasks) {
    const g: SessionGroup = groups.get(t.session)
      ?? { session: t.session, name: t.session_name, mine: t.session === id, tasks: [] }
    g.tasks.push(t)
    groups.set(t.session, g)
  }
  return [...groups.values()].sort((a, b) => Number(b.mine) - Number(a.mine)
    || Number(a.session === null) - Number(b.session === null)
    || a.name.localeCompare(b.name) || (a.session ?? '').localeCompare(b.session ?? ''))
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

function divider(g: SessionGroup, columns: number): string {
  const heading = `${g.mine ? '◆ ── This session ·' : '○ ──'} ${g.name} ──`
  return clip(heading + '─'.repeat(Math.max(0, columns - textWidth(heading))), columns)
}

function identity(t: Task, mine: boolean, columns: number) {
  const parts = [
    { text: `${mine ? '◆' : '○'} ` },
    { text: t.id, bold: true },
    { text: ` ${t.title}` },
    { text: t.worker ? `  ${t.worker}` : '', dimColor: true },
  ]
  const clipped = clip(parts.map(p => p.text).join(''), columns)
  let offset = 0
  return parts.map(p => {
    const text = clipped.slice(offset, offset + p.text.length)
    offset += p.text.length
    return { ...p, text }
  }).filter(p => p.text)
}

type SummaryPart = { text: string; color?: ThemeKey; other?: boolean }

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

function rowSummary(tasks: Task[], id: string | null): SummaryPart[] {
  const parts = summary(id === null ? tasks : tasks.filter(t => t.session === id))
  if (id === null) return parts
  const groups = new Map<string | null, { name: string; count: number; stalled: number }>()
  for (const t of tasks) {
    if (t.session === id || t.phase === 'not_started' || (t.phase === 'done' && t.outcome !== 'pass')) continue
    const g = groups.get(t.session) ?? { name: t.session === null ? 'by hand' : t.session_name, count: 0, stalled: 0 }
    g.count += 1
    g.stalled += Number(t.stalled)
    groups.set(t.session, g)
  }
  // Rust's string order compares Unicode scalar values, not the host's locale.
  const compare = (a: string, b: string): number => {
    const x = Array.from(a), y = Array.from(b)
    for (let i = 0; i < Math.min(x.length, y.length); i++) {
      const order = (x[i].codePointAt(0) ?? 0) - (y[i].codePointAt(0) ?? 0)
      if (order) return order
    }
    return x.length - y.length
  }
  const other = [...groups.entries()].sort(([a, x], [b, y]) => Number(a === null) - Number(b === null)
    || compare(x.name, y.name) || compare(a ?? '', b ?? ''))
    .map(([, g]) => `${g.name} ${g.count}${g.stalled ? ` (${g.stalled} stalled)` : ''}`).join(' · ')
  if (other) parts.push({ text: `${parts.length > 1 ? ' ' : ''}│ ${other}`, other: true })
  return parts
}

function textWidth(text: string): number {
  return Array.from(graphemes.segment(text), p => cellWidth(p.segment)).reduce((a, b) => a + b, 0)
}

function clipSummary(parts: SummaryPart[], columns: number): SummaryPart[] {
  if (columns <= 0) return []
  if (parts.reduce((n, p) => n + textWidth(p.text), 0) <= columns) return parts
  if (parts.length > 1 && parts[parts.length - 1].other) {
    const own = parts.slice(0, -1)
    const left = columns - own.reduce((n, p) => n + textWidth(p.text), 0)
    // An ellipsis for the other groups must never displace a count that fits.
    return left >= 0 ? [...own, ...clipSummary(parts.slice(-1), left)] : clipSummary(own, columns)
  }
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
    $.clock.every(EVERY_MS, () => { void refresh($) })
    return ran
  })

  on('command.run', { command: 'delegate-pane' }, async $ => {
    await $.ui.open({ id: PANE, title: 'Delegate tasks' })
    void refresh($)
    return { text: 'Delegate tasks pane opened.' }
  })

  // The pane registry isn't an atom: redraw the band after any open or close.
  on('ui.open', { id: PANE }, async ($, e, next) => {
    const result = await next(e)
    $.ui.invalidate('ui.render')
    return result
  })

  on('ui.close', { id: PANE }, async ($, e, next) => {
    const closed = await next(e)
    $.ui.invalidate('ui.render')
    return closed
  })

  on('ui.render', { component: 'AbovePrompt' }, async ($, e, next) => {
    const v = await read($, view)
    if (e.props.hasSurvey || (await $.ui.panes()).some(p => p.id === PANE && p.isPlaced)
      || (!v.error && !v.tasks.some(t => t.phase !== 'not_started')))
      return next(e)
    const { Box, Button, Text } = $.ui.resolve(e)
    const normal = e.surface === 'terminal' && e.viewport?.isFullscreen !== true
    const columns = Math.max(0, e.props.bodyColumns)
    const label = normal ? 'Open' : 'Open tasks'
    const controlWidth = label.length + (e.surface === 'terminal' ? 4 : 0)
    const buttonWidth = columns >= controlWidth ? controlWidth : 0
    const id = await sessionId($)
    const source: SummaryPart[] = v.error ? [{ text: 'delegate: status unavailable' }]
      : rowSummary(v.tasks, id)
    const button = <Button key="delegate-open" label={label} autoFocus onPress={async () => {
      await $.ui.open({ id: PANE, title: 'Delegate tasks' })
    }} />
    // Remote surfaces use proportional fonts: only the app knows what fits.
    if (e.surface !== 'terminal')
      return (
        <Box flexDirection="row" flexWrap="nowrap" width={columns} height={1} overflow="hidden">
          <Box flexDirection="row" flexShrink={0}>
            {source.filter(p => !p.other).map(p => <Text color={p.color} dimColor={!p.color}>{p.text}</Text>)}
          </Box>
          {source.filter(p => p.other).map(p => (
            <Box minWidth={0} flexShrink={1}>
              <Text dimColor wrap="truncate-end">{p.text}</Text>
            </Box>
          ))}
          <Box flexDirection="row" flexShrink={0}>
            <Text dimColor>{'  '}</Text>
            {button}
          </Box>
        </Box>
      )
    const total = source.reduce((n, p) => n + textWidth(p.text), 0)
    // Reserve the control first; shorten the keyboard hint before clipping counts.
    let hint = normal ? '/delegate-pane or ctrl+x tab, Enter to open' : ''
    if (total + hint.length + buttonWidth + 4 > columns) hint = normal ? '/delegate-pane to open' : ''
    if (hint.length + buttonWidth + 4 > columns) hint = ''
    const tailWidth = (hint ? hint.length + 2 : 0) + (buttonWidth ? buttonWidth + 2 : 0)
    const parts = clipSummary(source, columns - tailWidth)
    return (
      <Box flexDirection="row" flexWrap="nowrap" width={columns} height={1} overflow="hidden">
        {parts.map(p => <Text color={p.color} dimColor={!p.color}>{p.text}</Text>)}
        {hint ? <Text dimColor>{`${parts.length ? '  ' : ''}${hint}`}</Text> : null}
        {buttonWidth ? <Text dimColor>{parts.length || hint ? '  ' : ''}</Text> : null}
        {/* Even with no room for its label, keep the terminal's keyboard target. */}
        {buttonWidth ? button : normal
          ? <Box width={0} height={1} flexShrink={0} overflow="hidden">{button}</Box> : null}
      </Box>
    )
  })

  on('ui.render', { component: 'Pane', requestId: PANE }, async ($, e) => {
    const { Box, Text } = $.ui.resolve(e)
    const v = await read($, view)
    const now = v.readAt || Date.now()
    const width = Math.max(0, e.props.bodyColumns)
    const columns = Math.max(0, width - 2)
    if (v.error) return <Box flexDirection="column"><Text dimColor>{clip(v.error, width)}</Text></Box>
    if (v.tasks.length === 0)
      return (
        <Box flexDirection="column">
          <Text>{clip('No delegate tasks yet.', width)}</Text>
          <Text dimColor>{clip('Start one with: delegate new t001', width)}</Text>
        </Box>
      )
    const groups = sessions(v.tasks, await sessionId($))
    return (
      <Box flexDirection="column" width={width} overflow="hidden">
        {groups.map(g => (
          <Box key={`session:${g.session ?? 'unowned'}`} flexDirection="column">
            <Text bold={g.mine} dimColor={!g.mine} wrap="truncate-end">{divider(g, width)}</Text>
            {[...g.tasks].sort((a, b) => group(a) - group(b) || a.id.localeCompare(b.id)).map(t => {
              const l = label(t, now)
              const live = LIVE.has(t.phase)
              const n = t.changed_files.length
              const files = `${n} ${n === 1 ? 'file' : 'files'}`
              const bar: ThemeKey | undefined = t.stalled ? 'warning' : failed(t) ? 'error' : undefined
              const lines = [
                <Text color={l.color} bold={t.stalled}>{clip(l.text, columns)}</Text>,
                <Text>{identity(t, g.mine, columns).map(p => <Text bold={p.bold} dimColor={p.dimColor}>{p.text}</Text>)}</Text>,
              ]
              if (t.goal) lines.push(<Text dimColor>{clip(t.goal, columns)}</Text>)
              if (t.phase !== 'not_started')
                lines.push(<Text dimColor>{clip(live
                  ? `running ${ago(t.started_at, now)} · quiet ${ago(t.last_activity_at, now)} · ${n ? files : 'no files yet'}`
                  : `done · ${files}`, columns)}</Text>)
              if (live)
                for (const s of t.steps.slice(-3))
                  lines.push(<Text dimColor wrap="truncate-end">{clip(`  ${s.kind} ${s.target ?? ''} ${s.result ?? ''}`, columns)}</Text>)
              if ((t.stalled || failed(t) || t.ready_to_merge) && (t.next || t.stalled))
                lines.push(<Text>{clip(`→ ${t.next || `delegate status ${t.id}`}`, columns)}</Text>)
              return (
                <Box key={t.id} flexDirection="column" marginBottom={1}>
                  {lines.map(line => (
                    <Box flexDirection="row" width={width} flexWrap="nowrap">
                      <Box width={Math.min(1, width)} flexShrink={0} marginRight={width > 1 ? 1 : 0}>
                        <Text color={bar}>{clip(bar ? '┃' : ' ', Math.min(1, width))}</Text>
                      </Box>
                      <Box minWidth={0} flexShrink={1}>{line}</Box>
                    </Box>
                  ))}
                </Box>
              )
            })}
          </Box>
        ))}
      </Box>
    )
  })
}
