import { expect, test } from 'claude-code/testing'

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
const STATUS = {
  tasks: [
    {
      id: 't001', phase: 'implement', outcome: null, worker: 'pi:gpt-x@high', started_at: now,
      last_activity_at: now, stalled: false, changed_files: [{ path: 'shop/pricing.py' }],
      steps: [{ kind: 'run', target: 'python3 -m unittest -q', result: 'ok' }], ready_to_merge: false, next: null,
    },
    {
      id: 't002', phase: 'implement', outcome: null, worker: 'claude:sonnet@high', started_at: now,
      last_activity_at: now, stalled: true, changed_files: [], steps: [], ready_to_merge: false, next: null,
    },
    {
      id: 't003', phase: 'done', outcome: 'pass', worker: 'pi:gpt-x@high', started_at: now,
      last_activity_at: now, stalled: false, changed_files: [], steps: [], ready_to_merge: true,
      next: 'delegate merge t003',
    },
  ],
}

function engine(on: any, stdout: string, exitCode = 0) {
  on('command.register', () => ({ value: { command: 'delegate-pane' } }))
  on('ui.open', () => ({ value: { isPlaced: true } }))
  on('session.start', (_: unknown, e: { cwd: string }) => ({ cwd: e.cwd }))
  on('process.run', () => ({ value: { exitCode, stdout, stderr: exitCode ? 'error: not inside a git repository' : '' } }))
}

test('the pane shows each task, its steps, and a stalled one stands out', async ($, on) => {
  engine(on, JSON.stringify(STATUS))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  for (const surface of ['terminal', 'desktop'] as const) {
    const ui = await $.ui.mount({ ...PANE, surface })
    expect((await ui.find({ type: 'Text', text: /pricing\.py/ }))?.text).toContain('1 files')
    expect(await ui.find({ type: 'Text', text: /unittest/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /implement, stalled/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /ready to merge/ })).toBeTruthy()
    expect(await ui.find({ type: 'Text', text: /next: delegate merge t003/ })).toBeTruthy()
    await ui.unmount()
  }
})

test('no tasks: it says so and how to start one', async ($, on) => {
  engine(on, JSON.stringify({ tasks: [] }))
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /No delegate tasks yet/ })).toBeTruthy()
  await ui.unmount()
})

test('outside a project: the reason, not a broken pane', async ($, on) => {
  engine(on, '', 1)
  await $.session.start({ cwd: '/', surface: 'desktop', isInteractive: true })
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop' })
  expect(await ui.find({ type: 'Text', text: /not inside a git repository/ })).toBeTruthy()
  await ui.unmount()
})
