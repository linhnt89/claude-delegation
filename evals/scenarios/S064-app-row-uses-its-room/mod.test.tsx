import { expect, test } from 'claude-code/testing'
import { host, mock, rowOf, START, SURFACES, task } from './mod-host'

const TASKS = [
  task('t021'),
  task('t022', { phase: 'done', outcome: 'pass', ready_to_merge: true }),
  task('t030', { session: 'b', session_name: 'pane-polish' }),
  task('t032', { session: 'b', session_name: 'pane-polish', stalled: true }),
  task('t031', { session: null, session_name: 'by hand' }),
]

async function appRow($: any, on: any, columns: number) {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: TASKS })
  await $.session.start(START)
  await clock.advance(3000)
  seen.userCloses()
  const ui = await $.ui.mount({
    plugin: 'delegate-pane', component: 'AbovePrompt', requestId: 'delegate-band', surface: 'desktop',
    props: { hasSurvey: false, isWorking: false, maxRows: 10, bodyColumns: columns, scroll: { offset: 0, bodyRows: 10 }, view: {} },
  })
  const texts = await ui.findAll({ type: 'Text' })
  const button = await ui.find({ type: 'Button' })
  await ui.unmount()
  return { text: texts.map((t: any) => t.text).join(''), texts, button }
}

for (const columns of [65, 50]) {
  test(`in the desktop app at ${columns} reported columns the mod hands over every group in full, with no … of its own`, async ($, on) => {
    const r = await appRow($, on, columns)
    expect(r.text).toContain('✓ 1 ready to merge')
    expect(r.text).toContain('● 1 running')
    expect(r.text).toContain('│ pane-polish 2 (1 stalled) · by hand 1')
    expect(r.text).not.toContain('…')
    expect(r.button).toBeTruthy()
  })
}

test('in the desktop app only the other groups may be cut by the app: they truncate at the edge, the own counts do not', async ($, on) => {
  const r = await appRow($, on, 50)
  const others = r.texts.filter((t: any) => t.text.includes('pane-polish'))
  expect(others.length).toBeGreaterThan(0)
  for (const t of others) expect(t.props.wrap).toBe('truncate-end')
  for (const t of r.texts.filter((t: any) => /ready to merge|running/.test(t.text)))
    expect(t.props.wrap === 'truncate-end').toBe(false)
})

test('in the terminal the row is still cut by its exact columns, other groups first', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: TASKS })
  await $.session.start(START)
  await clock.advance(3000)
  seen.userCloses()
  const r = await rowOf($, SURFACES[1], 58)
  expect(r?.text ?? '(no row)').toContain('1 ready to merge')
  expect(r?.text ?? '').not.toContain('by hand 1')
})
