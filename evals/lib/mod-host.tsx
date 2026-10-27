// A stand-in for Claude Code and `delegate` around the pane mod, for scenario checks (mod.sh).
// It answers the calls the mod makes the way the host would, records what the mod asked for,
// and lets a check make `delegate status --json` slow or failing.
import { mock } from 'claude-code/testing'

export type Task = Record<string, any>

export const PANE = {
  plugin: 'delegate-pane', component: 'Pane', requestId: 'delegate',
  props: {
    title: 'Delegate tasks', isFocused: false, bodyColumns: 60, placement: 'dock',
    scroll: { offset: 0, bodyRows: 30 }, view: {},
  },
} as const

export const ROW = {
  plugin: 'delegate-pane', component: 'AbovePrompt', requestId: 'delegate-band',
  props: {
    hasSurvey: false, isWorking: false, maxRows: 10, bodyColumns: 200,
    scroll: { offset: 0, bodyRows: 10 }, view: {},
  },
} as const

// Where the row is drawn: the desktop app, and the terminal (fullscreen and normal screen).
export const SURFACES = [
  { surface: 'desktop', viewport: undefined, name: 'the desktop app' },
  { surface: 'terminal', viewport: { columns: 200, rows: 40, isFullscreen: true }, name: 'the fullscreen terminal' },
  { surface: 'terminal', viewport: { columns: 200, rows: 40, isFullscreen: false }, name: 'the terminal' },
] as const

const now = new Date().toISOString()
// task <id> [fields]: one task as `delegate status --json` gives it, running in session "a".
export function task(id: string, fields: Task = {}): Task {
  return {
    id, title: `Task ${id}`, goal: '', session: 'a', session_name: 'shop',
    phase: 'implement', outcome: null, worker: 'pi:gpt-x@high', started_at: now,
    last_activity_at: now, activity_age_secs: 0, elapsed_secs: 0, stalled: false,
    changed_files: [], steps: [], ready_to_merge: false, merge_blockers: [], next: null,
    pid: 1, ...fields,
  }
}

const HOST = '(drawn by Claude Code, not the mod)'

export function host(on: any, opts: { session?: string | null; tasks?: Task[] } = {}) {
  const session = opts.session === undefined ? 'a' : opts.session
  // Not `h`: that name is the JSX factory.
  const seen = {
    tasks: opts.tasks ?? [] as Task[],
    open: false, opened: [] as string[], closed: [] as string[],
    slow: false, fail: false, reads: 0, inflight: 0, maxInflight: 0,
    pending: [] as (() => void)[],
    // Let every slow read answer.
    release() { this.pending.splice(0).forEach(r => r()) },
    // The user closes the pane with its own close control.
    userCloses() { this.open = false },
  }
  on('command.register', () => ({ value: { command: 'delegate-pane' } }))
  on('ui.open', (_: unknown, e: { id: string }) => {
    seen.open = true
    seen.opened.push(e.id)
    return { value: { isPlaced: true } }
  })
  on('ui.panes', () => ({
    value: seen.open ? [{ id: 'delegate', title: 'Delegate tasks', isPlaced: true, isShown: true, isFocused: false }] : [],
  }))
  on('ui.close', (_: unknown, e: { id: string }) => {
    seen.open = false
    seen.closed.push(e.id)
    return { value: undefined }
  })
  on('session.id', () => {
    if (session === null) throw new Error('session id unavailable')
    return { value: session }
  })
  on('session.start', (_: unknown, e: { cwd: string }) => ({ cwd: e.cwd }))
  // What Claude Code draws when the mod leaves a surface to it.
  on('ui.render', (render: any, e: any) => {
    const { Text } = render.ui.resolve(e)
    return <Text>{HOST}</Text>
  })
  on('process.run', async () => {
    seen.reads += 1
    seen.inflight += 1
    seen.maxInflight = Math.max(seen.maxInflight, seen.inflight)
    if (seen.slow) await new Promise<void>(r => seen.pending.push(r))
    seen.inflight -= 1
    return {
      value: seen.fail
        ? { exitCode: 1, stdout: '', stderr: 'error: could not read the task state' }
        : { exitCode: 0, stdout: JSON.stringify({ tasks: seen.tasks }), stderr: '' },
    }
  })
  return seen
}

export const START = { cwd: '/', surface: 'desktop', isInteractive: true } as const

// The command the user types, or the row's button runs: `/delegate-pane`.
export function openCommand() {
  return {
    command: 'delegate-pane', args: '', origin: { kind: 'composer' as const },
    presentation: { isFullscreen: false, columns: 80 },
  }
}

// rowOf: the row above the prompt as drawn, its text and its pieces; null when hidden.
export async function rowOf($: any, where: { surface: string; viewport?: unknown } = SURFACES[0], bodyColumns = 200) {
  const ui = await $.ui.mount({ ...ROW, surface: where.surface, viewport: where.viewport, props: { ...ROW.props, bodyColumns } })
  const texts = await ui.findAll({ type: 'Text' })
  await ui.unmount()
  if (!texts.length || texts.every((t: any) => t.text === HOST)) return null
  return { text: texts.map((t: any) => t.text).join(''), parts: texts }
}

// paneText: everything the pane shows, one line per Text.
export async function paneText($: any): Promise<string> {
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  const texts = await ui.findAll({ type: 'Text' })
  await ui.unmount()
  return texts.map((t: any) => t.text).join('\n')
}

export { mock }
