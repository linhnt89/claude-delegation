import { expect, mock, test } from 'claude-code/testing'

import type { Task } from '../types'
import { register } from './register'

const PANE = {
  plugin: 'delegate-pane',
  component: 'Pane',
  requestId: 'delegate',
  props: {
    title: 'Delegate tasks',
    isFocused: false,
    bodyColumns: 60,
    placement: 'dock',
    scroll: { offset: 0, bodyRows: 30 },
    view: {},
  },
} as const

const now = new Date().toISOString()
function task(id: string, overrides: Partial<Task> = {}): Task {
  return {
    id, phase: 'implement', outcome: null, worker: 'pi:gpt-x@high', started_at: now,
    last_activity_at: now, stalled: false, changed_files: [], steps: [],
    ready_to_merge: false, merge_blockers: [], next: null, ...overrides,
  }
}

const STATUS = {
  tasks: [
    task('t001', {
      changed_files: [{ path: 'shop/pricing.py' }],
      steps: [{ kind: 'run', target: 'python3 -m unittest -q', result: 'ok' }],
    }),
    task('t002', { stalled: true, worker: 'claude:sonnet@high' }),
    task('t003', { phase: 'done', outcome: 'pass', ready_to_merge: true, next: 'delegate merge t003' }),
  ],
}

function engine(on: any, stdout: string, exitCode = 0) {
  let open = false
  const opened: string[] = []
  const closed: string[] = []
  let closeHook: any
  // The test engine has no surface-close entry point; dispatch the registered hook.
  register(((event: string, ...args: any[]) => {
    if (event === 'ui.close') closeHook = args[args.length - 1]
  }) as any)
  on('command.register', () => ({ value: { command: 'delegate-pane' } }))
  on('ui.open', (_: unknown, e: { id: string }) => {
    open = true
    opened.push(e.id)
    return { value: { isPlaced: true } }
  })
  on('ui.panes', () => ({ value: open ? [{ id: 'delegate', title: 'Delegate tasks', isPlaced: true, isShown: true, isFocused: false }] : [] }))
  on('ui.close', (_: unknown, e: { origin: { kind: string } }) => {
    open = false
    closed.push(e.origin.kind)
    return { value: undefined }
  })
  on('session.start', (_: unknown, e: { cwd: string }) => ({ cwd: e.cwd }))
  on('process.run', () => ({ value: { exitCode, stdout, stderr: exitCode ? 'error: not inside a git repository' : '' } }))
  return {
    opened, closed,
    async personClose() {
      let state = { running: ['t001'], autoOpened: true }
      const fixture = { state: {
        get: async () => ({ value: state, version: 1 }),
        set: async (_: unknown, value: typeof state) => {
          state = value
          return { isSet: true, version: 2 }
        },
      } }
      await closeHook(fixture, { id: 'delegate', origin: { kind: 'person' } }, async () => {
        open = false
        closed.push('person')
      })
      expect(state).toEqual({ running: ['t001'], autoOpened: false })
    },
    status(tasks: Task[]) { stdout = JSON.stringify({ tasks }); exitCode = 0 },
    fail() { exitCode = 1 },
  }
}

function command() {
  return {
    command: 'delegate-pane', args: '', origin: { kind: 'composer' as const },
    presentation: { isFullscreen: false, columns: 80 },
  }
}

test('the pane shows each task, its steps, and a stalled one stands out', async ($, on) => {
  engine(on, JSON.stringify(STATUS))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(await ui.find({ type: 'Text', text: /running .* · quiet .* · 1 file$/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /unittest/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /^⚠ STALLED · quiet / })).toMatchObject({ props: { color: 'warning', bold: true } })
    expect(await ui.find({ type: 'Text', text: /^→ delegate status t002$/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /^✓ ready to merge$/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /^→ delegate merge t003$/ })).toBeTruthy()
    await ui.unmount()
  }
})

test('no tasks: it says so and how to start one', async ($, on) => {
  const e = engine(on, JSON.stringify({ tasks: [] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  expect(e.opened).toEqual([])
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /No delegate tasks yet/ })).toBeTruthy()
  expect(await ui.find({ type: 'Text', text: /Start one with: delegate new t001/ })).toBeTruthy()
  await ui.unmount()
})

test('outside a project: the reason, not a broken pane', async ($, on) => {
  engine(on, '', 1)
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /not inside a git repository/ })).toBeTruthy()
  await ui.unmount()
})

test('cards sort trouble, finished, running, then not started, by id within each group', async ($, on) => {
  const tasks = [
    task('t009', { phase: 'not_started' }),
    task('t008', { phase: 'review' }),
    task('t007', { phase: 'done', outcome: 'pass', merge_blockers: ['review required'] }),
    task('t006', { phase: 'done', outcome: 'pass', ready_to_merge: true }),
    task('t005', { phase: 'checks' }),
    task('t004', { phase: 'done', outcome: 'fail', next: 'delegate run t004' }),
    task('t003', { phase: 'interrupted', next: 'delegate run t003' }),
    task('t002', { stalled: true }),
    task('t001', { phase: 'questions' }),
  ]
  engine(on, JSON.stringify({ tasks }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    const cards = (await ui.findAll({ type: 'Box' })).filter(b => b.key?.startsWith('t'))
    expect(cards.map(c => c.key)).toEqual(['t002', 't003', 't004', 't006', 't007', 't001', 't005', 't008', 't009'])
    for (const text of ['✗ interrupted', '✗ failed', '✓ passed · review required', '● questions', '● checks', '● review', 'not started'])
      expect(await ui.find({ type: 'Text', text: new RegExp(`^${text}$`) })).toBeTruthy()
    const bars = await ui.findAll({ type: 'Text', text: /^┃$/ })
    expect(bars.filter(b => b.props.color === 'warning')).toHaveLength(4)
    expect(bars.filter(b => b.props.color === 'error')).toHaveLength(8)
    expect(await ui.findAll({ type: 'Text', text: /^ $/ })).toHaveLength(17)
    expect(await ui.find({ type: 'Text', text: /^→ delegate run t003$/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /^→ delegate run t004$/ })).toBeTruthy()
    for (const text of [/^⚠/, /^✗/, /^✓/, /^●/, /^not started$/, /^→/, /^t\d+$/])
      for (const line of await ui.findAll({ type: 'Text', text }))
        expect(Boolean(line.props.dimColor)).toBe(false)
    await ui.unmount()
  }
})

test('passed cards explain the first blocker or use the fallback; stalled cards use next when given', async ($, on) => {
  engine(on, JSON.stringify({ tasks: [
    task('t001', { phase: 'done', outcome: 'pass', merge_blockers: ['review required', 'dirty worktree'] }),
    task('t002', { phase: 'done', outcome: 'pass' }),
    task('t003', { stalled: true, next: 'inspect worker log' }),
  ] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /^✓ passed · review required$/ })).toBeTruthy()
  expect(await ui.find({ type: 'Text', text: /dirty worktree/ })).toBeUndefined()
  expect(await ui.find({ type: 'Text', text: /^✓ passed · not ready to merge$/ })).toBeTruthy()
  expect(await ui.find({ type: 'Text', text: /^→ inspect worker log$/ })).toBeTruthy()
  await ui.unmount()
})

test('only the latest three running steps are shown, clipped inside bodyColumns', async ($, on) => {
  engine(on, JSON.stringify({ tasks: [task('t001', { steps: [
    { kind: 'old', target: 'hidden' },
    { kind: 'run', target: 'x'.repeat(200), result: 'ok' },
    { kind: 'edit', target: '界'.repeat(80) },
    { kind: 'read', target: 'a\ncommand\twith spaces', result: 'ok' },
  ] }), task('t002', { phase: 'done', outcome: 'pass', steps: [{ kind: 'old', target: 'hidden' }] })] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    for (const bodyColumns of [20, 60]) {
      const ui = await $.ui.mount({ ...PANE, surface, props: { ...PANE.props, bodyColumns } })
      expect(await ui.find({ type: 'Text', text: /hidden/ })).toBeUndefined()
      const run = await ui.find({ type: 'Text', text: /^  run / })
      expect(run?.text).toBe(`  run ${'x'.repeat(bodyColumns - 9)}…`)
      const edit = await ui.find({ type: 'Text', text: /^  edit / })
      expect(edit?.text.endsWith('…')).toBe(true)
      expect(Array.from(edit?.text ?? '').reduce((n, c) => n + (c === '界' ? 2 : 1), 2) <= bodyColumns).toBe(true)
      expect((await ui.find({ type: 'Text', text: /^  read / }))?.text.includes('\n')).toBe(false)
      await ui.unmount()
    }
  }
})

test('the pane opens when a task starts, follows refreshes, and closes when only unstarted tasks remain', async ($, on) => {
  const clock = mock.clock(on)
  const e = engine(on, JSON.stringify({ tasks: [task('t001', { phase: 'not_started' })] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  expect(e.opened).toEqual([])
  e.status([task('t001')])
  await clock.advance(3000)
  expect(e.opened).toEqual(['delegate'])
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /^● implementing$/ })).toBeTruthy()
  e.status([task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true })])
  await clock.advance(3000)
  expect(await ui.find({ type: 'Text', text: /^✓ ready to merge$/ })).toBeTruthy()
  expect(e.closed).toEqual([])
  e.status([task('t002', { phase: 'not_started' })])
  await clock.advance(3000)
  expect(e.closed).toEqual(['plugin'])
  await clock.advance(3000)
  expect(e.closed).toEqual(['plugin'])
  await ui.unmount()
})

test('a person can close the pane without it reopening until another task starts', async ($, on) => {
  const clock = mock.clock(on)
  const e = engine(on, JSON.stringify(STATUS))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  expect(e.opened).toEqual(['delegate'])
  await e.personClose()
  await clock.advance(3000)
  e.status([task('t001', { phase: 'checks' })])
  await clock.advance(3000)
  expect(e.opened).toEqual(['delegate'])
  e.status([task('t001', { phase: 'checks' }), task('t004')])
  await clock.advance(3000)
  expect(e.opened).toEqual(['delegate', 'delegate'])
  e.status([])
  await clock.advance(3000)
  expect(e.closed).toEqual(['person', 'plugin'])
})

test('a command-opened pane stays open even when no tasks remain or another task starts', async ($, on) => {
  const clock = mock.clock(on)
  const e = engine(on, JSON.stringify({ tasks: [] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  await $.command.run(command())
  e.status([task('t001')])
  await clock.advance(3000)
  e.status([])
  await clock.advance(3000)
  expect(e.opened).toEqual(['delegate'])
  expect(e.closed).toEqual([])
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /No delegate tasks yet/ })).toBeTruthy()
  await ui.unmount()
})

test('the command takes ownership of an auto-opened pane', async ($, on) => {
  const clock = mock.clock(on)
  const e = engine(on, JSON.stringify(STATUS))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  await $.command.run(command())
  e.status([])
  await clock.advance(3000)
  expect(e.closed).toEqual([])
})

test('finished tasks alone do not auto-open, and a failed refresh does not close a live pane', async ($, on) => {
  const clock = mock.clock(on)
  const e = engine(on, JSON.stringify({ tasks: [task('t001', { phase: 'done', outcome: 'pass' })] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  expect(e.opened).toEqual([])
  e.status([task('t002')])
  await clock.advance(3000)
  e.fail()
  await clock.advance(3000)
  expect(e.closed).toEqual([])
  e.status([task('t002')])
  await clock.advance(3000)
  expect(e.opened).toEqual(['delegate'])
})
