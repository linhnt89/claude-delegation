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

const BAND = {
  plugin: 'delegate-pane',
  component: 'AbovePrompt',
  requestId: 'delegate-band',
  props: {
    hasSurvey: false, isWorking: false, maxRows: 10, bodyColumns: 200,
    scroll: { offset: 0, bodyRows: 10 }, view: {},
  },
} as const

const now = new Date().toISOString()
function task(id: string, overrides: Partial<Task> = {}): Task {
  return {
    id, title: `Task ${id}`, goal: '', session: 'session-a', session_name: 'my-project',
    phase: 'implement', outcome: null, worker: 'pi:gpt-x@high', started_at: now,
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

function engine(on: any, stdout: string, exitCode = 0, sessionId: string | null = 'session-a') {
  let open = false
  let isPlaced = true
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
    return { value: { isPlaced } }
  })
  on('ui.panes', () => ({ value: open ? [{ id: 'delegate', title: 'Delegate tasks', isPlaced, isShown: true, isFocused: false }] : [] }))
  on('ui.close', (_: unknown, e: { origin: { kind: string } }) => {
    open = false
    closed.push(e.origin.kind)
    return { value: undefined }
  })
  on('session.id', () => {
    if (sessionId === null) throw new Error('session id unavailable')
    return { value: sessionId }
  })
  on('session.start', (_: unknown, e: { cwd: string }) => ({ cwd: e.cwd }))
  on('process.run', () => ({ value: { exitCode, stdout, stderr: exitCode ? 'error: not inside a git repository' : '' } }))
  return {
    opened, closed,
    async personClose() {
      let state = { running: ['t001'], autoOpened: true }
      const fixture = { ui: { invalidate() {} }, state: {
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
    placed(value: boolean) { isPlaced = value },
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

function summaryEngine($: any, on: any, stdout: string, sessionId: string | null = 'session-a') {
  const e = engine(on, stdout, 0, sessionId)
  const closeHooks: any[] = []
  register(((event: string, ...args: any[]) => {
    if (event === 'ui.close') closeHooks.push(args[args.length - 1])
  }) as any)
  on('ui.render', ($: any, event: any) => {
    const { Text } = $.ui.resolve(event)
    return <Text>engine band</Text>
  })
  return {
    ...e,
    async change(kind: 'person' | 'plugin' | 'open', ui?: { redraw(): Promise<void> }) {
      if (kind === 'open') return $.command.run(command())
      // Like engine.personClose, dispatch the missing surface-close entry point.
      // Honor the mod's redraw request only after the registry has changed.
      let invalidated = false
      let state = { running: [], autoOpened: true }
      const fixture = {
        state: {
          get: async () => ({ value: state, version: 1 }),
          set: async (_: unknown, value: typeof state) => {
            state = value
            return { isSet: true, version: 2 }
          },
        },
        ui: { invalidate(event: string) { expect(event).toBe('ui.render'); invalidated = true } },
      }
      const event = { id: 'delegate', origin: { kind } }
      const run = (i: number): Promise<unknown> => i === closeHooks.length
        ? e.personClose() : closeHooks[i](fixture, event, () => run(i + 1))
      await run(0)
      expect(invalidated).toBe(true)
      expect(state.autoOpened).toBe(kind !== 'person')
      if (ui) await ui.redraw()
    },
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

test('session dividers put mine first, other sessions by name, and by hand last; cards retain priority and colours', async ($, on) => {
  mock.clock(on)
  engine(on, JSON.stringify({ tasks: [
    task('t001', { session: null, session_name: 'by hand', phase: 'interrupted' }),
    task('t002', { session: 'z', session_name: 'zebra', stalled: true }),
    task('t003', { session: 'b', session_name: 'alpha', phase: 'done', outcome: 'fail' }),
    task('t004', { session: 'b', session_name: 'alpha', phase: 'done', outcome: 'pass', ready_to_merge: true }),
    task('t005'),
    task('t006', { phase: 'not_started' }),
    task('t007', { stalled: true }),
    task('t008', { session: 'c', session_name: 'alpha' }),
    task('t009', { session: 'b', session_name: 'alpha' }),
  ] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    const dividers = (await ui.findAll({ type: 'Text' })).filter(t => /^[◆○] ──/.test(t.text))
    expect(dividers.map(d => d.text)).toEqual([
      `◆ ── This session · my-project ${'─'.repeat(29)}`,
      `○ ── alpha ${'─'.repeat(49)}`, `○ ── alpha ${'─'.repeat(49)}`,
      `○ ── zebra ${'─'.repeat(49)}`, `○ ── by hand ${'─'.repeat(47)}`,
    ])
    expect(dividers[0]).toMatchObject({ props: { bold: true, dimColor: false } })
    for (const d of dividers.slice(1)) expect(d).toMatchObject({ props: { bold: false, dimColor: true } })
    const boxes = await ui.findAll({ type: 'Box' })
    expect(boxes.filter(b => b.key?.startsWith('session:')).map(b => b.key))
      .toEqual(['session:session-a', 'session:b', 'session:c', 'session:z', 'session:unowned'])
    expect(boxes.filter(b => b.key?.startsWith('t')).map(b => b.key))
      .toEqual(['t007', 't005', 't006', 't003', 't004', 't009', 't008', 't002', 't001'])
    for (const id of ['t005', 't006', 't007'])
      expect(await ui.find({ type: 'Text', text: new RegExp(`^◆ ${id} Task ${id}  pi:gpt-x@high$`) })).toBeTruthy()
    for (const id of ['t001', 't002', 't003', 't004', 't008', 't009'])
      expect(await ui.find({ type: 'Text', text: new RegExp(`^○ ${id} Task ${id}  pi:gpt-x@high$`) })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: '✗ failed' })).toMatchObject({ props: { color: 'error' } })
    expect(await ui.find({ type: 'Text', text: '✓ ready to merge' })).toMatchObject({ props: { color: 'success' } })
    expect(await ui.find({ type: 'Text', text: '● implementing' })).toMatchObject({ props: { color: 'suggestion' } })
    const bars = await ui.findAll({ type: 'Text', text: '┃' })
    expect(bars.filter(b => b.props.color === 'warning')).toHaveLength(8)
    expect(bars.filter(b => b.props.color === 'error')).toHaveLength(6)
    await ui.unmount()
  }
})

test('a sole session still has a divider; identity has a bold id, title, dim worker, then a dim nonempty goal', async ($, on) => {
  mock.clock(on)
  engine(on, JSON.stringify({ tasks: [
    task('t001', { title: 'Fix pricing', goal: 'Charge the correct amount' }),
    task('t002', { worker: null, title: 'Add tests', goal: '' }),
  ] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    expect(await ui.find({ type: 'Text', text: /^◆ ── This session · my-project ─/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: '◆ t001 Fix pricing  pi:gpt-x@high' })).toBeTruthy()
    const identity = await ui.find({ type: 'Text', text: /^◆ t001 Fix pricing  pi:gpt-x@high$/ })
    expect(identity?.children[1]).toMatchObject({ props: { bold: true }, children: ['t001'] })
    expect(identity?.children[3]).toMatchObject({ props: { dimColor: true }, children: ['  pi:gpt-x@high'] })
    expect(await ui.find({ type: 'Text', text: 'Charge the correct amount' })).toMatchObject({ props: { dimColor: true } })
    const rows = (await ui.findAll({ type: 'Box' })).filter(b => b.props.flexDirection === 'row')
    expect(rows).toHaveLength(7)
    expect(rows.slice(0, 3).map(r => r.text.trimStart())).toEqual([
      '● implementing', '◆ t001 Fix pricing  pi:gpt-x@high', 'Charge the correct amount',
    ])
    expect(rows[3].text.trimStart()).toMatch(/^running .* · quiet .* · no files yet$/)
    expect(rows.slice(4, 6).map(r => r.text.trimStart())).toEqual(['● implementing', '◆ t002 Add tests'])
    expect(rows[6].text.trimStart()).toMatch(/^running .* · quiet .* · no files yet$/)
    expect(await ui.findAll({ type: 'Text', text: '┃' })).toHaveLength(0)
    await ui.unmount()
  }
})

test('without tasks of mine there is no empty current group, and a by-hand-only project still has its divider', async ($, on) => {
  const clock = mock.clock(on)
  const other = task('t001', { session: 'other', session_name: 'other project' })
  const manual = task('t002', { session: null, session_name: 'by hand' })
  const e = engine(on, JSON.stringify({ tasks: [manual, other] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /This session/ })).toBeUndefined()
  expect(await ui.find({ type: 'Text', text: /^○ ── other project ─/ })).toBeTruthy()
  e.status([manual])
  await clock.advance(3000)
  expect(await ui.find({ type: 'Text', text: /^○ ── by hand ─/ })).toBeTruthy()
  expect(await ui.find({ type: 'Text', text: /^○ t002 / })).toBeTruthy()
  expect(await ui.find({ type: 'Text', text: /This session|other project/ })).toBeUndefined()
  await ui.unmount()
})

function widthOf(text: string): number {
  return Array.from(new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text), ({ segment }) =>
    /^[\p{Mark}\p{Control}\p{Format}]+$/u.test(segment) ? 0
      : /[\p{Extended_Pictographic}界]/u.test(segment) ? 2 : 1).reduce((a, b) => a + b, 0)
}

test('every divider and card line clips to pane width, including Unicode, status, identity, goal, timing, steps, and next', async ($, on) => {
  mock.clock(on)
  const long = `界👩‍💻é${'x'.repeat(200)}`
  engine(on, JSON.stringify({ tasks: [
    task('t001', {
      title: long, goal: `${long}\nmore\tgoal`, worker: long, session_name: long,
      stalled: true, next: long, steps: [{ kind: 'run', target: long }],
    }),
    task('t002', { phase: 'done', outcome: 'pass', merge_blockers: [long], title: long, worker: long }),
  ] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    for (const bodyColumns of [0, 1, 2, 3, 4, 10, 20, 60, 200]) {
      const ui = await $.ui.mount({ ...PANE, surface, props: { ...PANE.props, bodyColumns } })
      const rows = (await ui.findAll({ type: 'Box' })).filter(b => b.props.flexDirection === 'row')
      expect(rows.length > 0).toBe(true)
      for (const row of rows) {
        // Render-tree text omits the gutter's one-column right margin.
        expect(widthOf(row.text) + (bodyColumns > 1 ? 1 : 0) <= bodyColumns).toBe(true)
        expect(/[\r\n\t]/.test(row.text)).toBe(false)
      }
      const dividers = (await ui.findAll({ type: 'Text' })).filter(t => t.props.wrap === 'truncate-end' && !t.props.dimColor)
      expect(dividers).toHaveLength(1)
      expect(widthOf(dividers[0].text) <= bodyColumns).toBe(true)
      if (bodyColumns >= 20) {
        // Long identity, goal, step, next, blocker/status, and second identity lines.
        for (const index of [1, 2, 4, 5, 6, 7]) expect(rows[index].text.endsWith('…')).toBe(true)
        if (bodyColumns === 20) expect(rows[3].text.endsWith('…')).toBe(true)
      }
      await ui.unmount()
    }
  }
})

test('an unreadable session id treats all cards as mine in one priority-sorted group', async ($, on) => {
  mock.clock(on)
  engine(on, JSON.stringify({ tasks: [
    task('t001', { session: 'b', session_name: 'Beta', title: 'Running elsewhere', goal: 'Keep the goal' }),
    task('t002', { session: null, session_name: 'by hand', stalled: true }),
    task('t003', { phase: 'not_started' }),
  ] }), 0, null)
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    const groups = (await ui.findAll({ type: 'Box' })).filter(b => b.key?.startsWith('session:'))
    expect(groups).toHaveLength(1)
    expect((await ui.findAll({ type: 'Box' })).filter(b => b.key?.startsWith('t')).map(b => b.key))
      .toEqual(['t002', 't001', 't003'])
    for (const id of ['t001', 't002', 't003'])
      expect(await ui.find({ type: 'Text', text: new RegExp(`^◆ ${id} `) })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /^○/ })).toBeUndefined()
    expect(await ui.find({ type: 'Text', text: 'Keep the goal' })).toMatchObject({ props: { dimColor: true } })
    await ui.unmount()
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

const HINT = '/delegate-pane or ctrl+x tab, Enter to open'
const SHORT_HINT = '/delegate-pane to open'

for (const { surface, viewport, name, normal } of [
  { surface: 'desktop', viewport: undefined, name: 'desktop', normal: false },
  { surface: 'terminal', viewport: { columns: 200, rows: 40, isFullscreen: true }, name: 'fullscreen terminal', normal: false },
  { surface: 'terminal', viewport: { columns: 200, rows: 40, isFullscreen: false }, name: 'normal terminal', normal: true },
  { surface: 'terminal', viewport: undefined, name: 'unmeasured terminal', normal: true },
] as const) {
  test(`summary counts trouble first, once per task, with semantic colours on ${name}`, async ($, on) => {
    mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [
      task('t009', { phase: 'not_started' }),
      task('t008', { phase: 'checks' }), task('t007', { phase: 'review' }),
      task('t006', { phase: 'done', outcome: 'pass' }),
      task('t005', { phase: 'done', outcome: 'pass', ready_to_merge: true }),
      task('t004', { phase: 'done', outcome: 'fail' }),
      task('t003', { phase: 'interrupted' }),
      task('t002', { phase: 'interrupted', stalled: true }),
      task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true, stalled: true }),
    ] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    await e.change('person')
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    const row = await ui.find({ type: 'Box' })
    expect(row).toMatchObject({ props: { flexDirection: 'row', height: 1, flexWrap: 'nowrap' } })
    const button = await ui.find({ type: 'Button', key: 'delegate-open' })
    expect(button?.text).toBe(normal ? 'Open' : 'Open tasks')
    expect(button?.props.autoFocus).toBe(true)
    expect(button?.props.plain).toBeUndefined()
    expect(button?.children).toEqual([])
    expect(await ui.findAll({ type: 'Button' })).toHaveLength(1)
    const texts = await ui.findAll({ type: 'Text' })
    expect(texts.map(t => t.text).join('')).toBe(`delegate  ⚠ 2 stalled · ✗ 1 interrupted · ✗ 1 failed · ✓ 1 ready to merge · ✓ 1 passed · ● 2 running · 1 not started${normal ? `  ${HINT}` : ''}  `)
    // Counts are direct Text children of the row, never Button children.
    expect(row?.children.filter((c: any) => c.type === 'Text')).toHaveLength(texts.length)
    for (const [text, color] of [
      ['⚠ 2 stalled', 'warning'], ['✗ 1 interrupted', 'error'], ['✗ 1 failed', 'error'],
      ['✓ 1 ready to merge', 'success'], ['✓ 1 passed', 'success'], ['● 2 running', 'suggestion'],
      ['1 not started', 'inactive'],
    ])
      expect(await ui.find({ type: 'Text', text })).toMatchObject({ props: { color, dimColor: false } })
    for (const text of ['delegate  ', ' · ', ...(normal ? [HINT] : [])])
      expect(await ui.find({ type: 'Text', text })).toMatchObject({ props: { dimColor: true } })
    await ui.unmount()
  })

  test(`summary counts only my session but remains visible for actionable tasks elsewhere on ${name}`, async ($, on) => {
    const clock = mock.clock(on)
    const mine = task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true })
    const other = task('t002', { session: 'other', session_name: 'other', stalled: true })
    const manual = task('t003', { session: null, session_name: 'by hand', phase: 'done', outcome: 'fail' })
    const e = summaryEngine($, on, JSON.stringify({ tasks: [mine, other, manual] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    await e.change('person')
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    expect(await ui.find({ type: 'Text', text: '✓ 1 ready to merge' })).toMatchObject({ props: { color: 'success' } })
    expect(await ui.find({ type: 'Text', text: /stalled|failed/ })).toBeUndefined()
    e.status([task('t001', { phase: 'not_started' }), other])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Text', text: '1 not started' })).toBeTruthy()
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    e.status([other, manual])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Text', text: 'delegate  ' })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /stalled|failed|not started|ready to merge/ })).toBeUndefined()
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    e.status([task('t001', { phase: 'not_started' })])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await ui.unmount()
  })

  test(`summary falls back to counting every session when its id is unreadable on ${name}`, async ($, on) => {
    mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [
      task('t001', { session: 'other', session_name: 'other', stalled: true }),
      task('t002', { session: null, session_name: 'by hand', phase: 'done', outcome: 'fail' }),
      task('t003', { phase: 'done', outcome: 'pass', ready_to_merge: true }),
    ] }), null)
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    await e.change('person')
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    expect(await ui.find({ type: 'Text', text: '⚠ 1 stalled' })).toMatchObject({ props: { color: 'warning' } })
    expect(await ui.find({ type: 'Text', text: '✗ 1 failed' })).toMatchObject({ props: { color: 'error' } })
    expect(await ui.find({ type: 'Text', text: '✓ 1 ready to merge' })).toMatchObject({ props: { color: 'success' } })
    await ui.unmount()
  })

  test(`summary yields for empty, unstarted, open panes and surveys on ${name}`, async ($, on) => {
    const clock = mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    expect(await ui.find({ type: 'Text', text: 'engine band' })).toBeTruthy()
    e.status([task('t001', { phase: 'not_started' })])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    e.status([task('t001')])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await e.change('person', ui)
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    await ui.redraw({ ...BAND.props, hasSurvey: true })
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    expect(await ui.find({ type: 'Text', text: 'engine band' })).toBeTruthy()
    await ui.redraw({ ...BAND.props, isWorking: true })
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    e.status([])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await ui.unmount()
  })

  test(`summary shows while the auto-opened pane is listed but not placed on ${name}`, async ($, on) => {
    mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify(STATUS))
    e.placed(false)
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    expect(e.opened).toEqual(['delegate'])
    const ui = await $.ui.mount({
      ...BAND, surface,
      viewport: viewport ? { ...viewport, columns: 130 } : undefined,
      props: { ...BAND.props, bodyColumns: 125 },
    })
    expect(await ui.find({ type: 'Text', text: /delegate/ })).toBeTruthy()
    expect(await ui.find({ type: 'Button', key: 'delegate-open' })).toBeTruthy()
    e.placed(true)
    await ui.press({ key: 'delegate-open' })
    expect(e.opened).toEqual(['delegate', 'delegate'])
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await ui.unmount()
  })

  test(`summary press opens a person-owned pane; manual and mod closes redraw on ${name}`, async ($, on) => {
    const clock = mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true })] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    expect(await ui.find({ type: 'Button' })).toMatchObject({ text: normal ? 'Open' : 'Open tasks', props: { autoFocus: true } })
    await ui.press({ key: 'delegate-open' })
    expect(e.opened).toEqual(['delegate'])
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    // A summary-opened pane stays open, like a command-opened one.
    e.status([])
    await clock.advance(3000)
    expect(e.closed).toEqual([])
    e.status([task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true })])
    await clock.advance(3000)
    await e.change('person', ui)
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    await e.change('open')
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await e.change('plugin', ui)
    expect(await ui.find({ type: 'Button' })).toBeTruthy()
    await ui.unmount()
  })

  test(`summary follows status refreshes and reports failures on ${name}`, async ($, on) => {
    const clock = mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [task('t001', { phase: 'done', outcome: 'pass' })] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    const ui = await $.ui.mount({ ...BAND, surface, viewport })
    expect(await ui.find({ type: 'Text', text: '✓ 1 passed' })).toBeTruthy()
    e.status([task('t001', { phase: 'done', outcome: 'fail' })])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Text', text: '✗ 1 failed' })).toBeTruthy()
    e.fail()
    await clock.advance(3000)
    expect(await ui.find({ type: 'Text', text: /^delegate: status unavailable$/ })).toMatchObject({ props: { dimColor: true } })
    expect((await ui.find({ type: 'Button' }))?.text).toBe(normal ? 'Open' : 'Open tasks')
    if (normal) expect(await ui.find({ type: 'Text', text: HINT })).toMatchObject({ props: { dimColor: true } })
    for (const text of await ui.findAll({ type: 'Text' }))
      expect(text.props.dimColor).toBe(true)
    await ui.redraw({ ...BAND.props, hasSurvey: true })
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await ui.redraw(BAND.props)
    await ui.press({ key: 'delegate-open' })
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await e.change('person', ui)
    expect(await ui.find({ type: 'Text', text: /^delegate: status unavailable$/ })).toBeTruthy()
    e.status([])
    await clock.advance(3000)
    expect(await ui.find({ type: 'Button' })).toBeUndefined()
    await ui.unmount()
  })

  test(`summary stays one row, preserving the button and shortening the hint on ${name}`, async ($, on) => {
    mock.clock(on)
    const e = summaryEngine($, on, JSON.stringify({ tasks: [task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true })] }))
    await $.session.start({ cwd: '/', surface, isInteractive: true })
    for (const bodyColumns of [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 20, 29, 31, 32, 40, 60, 100]) {
      const ui = await $.ui.mount({ ...BAND, surface, viewport, props: { ...BAND.props, bodyColumns } })
      const button = await ui.find({ type: 'Button', key: 'delegate-open' })
      const buttonWidth = (normal ? 4 : 10) + (surface === 'terminal' ? 4 : 0)
      const shownButton = bodyColumns >= buttonWidth
      expect(Boolean(button)).toBe(normal || shownButton)
      if (button) {
        expect(button.props.autoFocus).toBe(true)
        expect(button.props.plain).toBeUndefined()
      }
      const text = (await ui.findAll({ type: 'Text' })).map(t => t.text).join('')
      const width = Array.from(new Intl.Segmenter(undefined, { granularity: 'grapheme' }).segment(text), p =>
        /\p{Extended_Pictographic}/u.test(p.segment) ? 2 : 1).reduce((a, b) => a + b, 0)
        + (shownButton ? buttonWidth : 0)
      expect(width <= bodyColumns).toBe(true)
      expect(/[\r\n\t]/.test(text)).toBe(false)
      expect(await ui.find({ type: 'Box' })).toMatchObject({ props: { width: bodyColumns, height: 1, overflow: 'hidden' } })
      if (bodyColumns === 20) expect(text).toBe(normal ? 'delegate …  '
        : surface === 'terminal' ? 'del…  ' : 'delegat…  ')
      if (bodyColumns === 60) expect(text).toBe(normal
        ? `delegate  ✓ 1 ready to me…  ${SHORT_HINT}  ` : 'delegate  ✓ 1 ready to merge  ')
      if (bodyColumns === 100 && normal) expect(text).toContain(HINT)
      if (bodyColumns === 40 && normal) expect(text).toContain(`…  ${SHORT_HINT}`)
      if (!shownButton && normal)
        expect(await ui.find({ type: 'Box', text: 'Open' })).toBeTruthy()
      if (button) {
        await ui.press({ key: 'delegate-open' })
        expect(await ui.find({ type: 'Button' })).toBeUndefined()
        await e.change('plugin', ui)
      }
      await ui.unmount()
    }
  })
}
