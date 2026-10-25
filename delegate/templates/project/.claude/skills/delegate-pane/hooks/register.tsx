import { atom, read, update } from 'claude-code'
import type { EngineInterface, Register } from 'claude-code'

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
  return next
}

function ago(iso: string | null, now: number): string {
  if (!iso) return '-'
  const s = Math.max(0, Math.round((now - Date.parse(iso)) / 1000))
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m${s % 60}s` : `${Math.floor(s / 3600)}h${Math.floor((s % 3600) / 60)}m`
}

function label(t: Task): { text: string; color?: string } {
  if (t.stalled) return { text: `${t.phase}, stalled`, color: 'warning' }
  if (t.phase === 'interrupted') return { text: 'interrupted', color: 'error' }
  if (t.phase === 'done' && t.outcome === 'pass')
    return { text: t.ready_to_merge ? 'done, pass, ready to merge' : 'done, pass', color: 'success' }
  if (t.phase === 'done') return { text: `done, ${t.outcome ?? '-'}`, color: t.outcome === 'fail' ? 'error' : undefined }
  if (t.phase === 'not_started') return { text: 'not started', color: 'inactive' }
  return { text: t.phase, color: 'suggestion' }
}

export const register: Register = on => {
  on('session.start', async ($, e, next) => {
    const ran = await next(e)
    await $.command.register({ name: 'delegate-pane', description: 'Show delegate tasks in a pane' })
    const first = await refresh($)
    // Open by itself only where there is something to follow.
    if (first.tasks.some(t => t.phase !== 'not_started')) void $.ui.open({ id: PANE, title: 'Delegate tasks' })
    $.clock.every(EVERY_MS, () => refresh($))
    return ran
  })

  on('command.run', { command: 'delegate-pane' }, async $ => {
    await refresh($)
    await $.ui.open({ id: PANE, title: 'Delegate tasks' })
    return { text: 'Delegate tasks pane opened.' }
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
        {v.tasks.map(t => {
          const l = label(t)
          const files = t.changed_files.map(f => f.path)
          const live = LIVE.has(t.phase)
          return (
            <Box flexDirection="column" marginBottom={1}>
              <Text>
                <Text bold>{t.id}</Text> <Text color={l.color}>{l.text}</Text>
                {t.worker ? <Text dimColor>  {t.worker}</Text> : null}
              </Text>
              {t.phase !== 'not_started' && (
                <Text dimColor>
                  {live ? `running ${ago(t.started_at, now)}, ` : ''}last activity {ago(t.last_activity_at, now)} ago
                  {files.length ? `, ${files.length} files: ${files.slice(0, 3).join(', ')}${files.length > 3 ? ', …' : ''}` : ''}
                </Text>
              )}
              {live &&
                t.steps.slice(-3).map(s => (
                  <Text dimColor>
                    {'  '}
                    {s.kind} {s.target ?? ''} {s.result ?? ''}
                  </Text>
                ))}
              {!live && t.next && <Text dimColor>  next: {t.next}</Text>}
            </Box>
          )
        })}
      </Box>
    )
  })
}
