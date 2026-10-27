import { expect, test } from 'claude-code/testing'
import { host, mock, openCommand, paneText, rowOf, START, SURFACES, task } from './mod-host'

test('no pane opens by itself when a task starts, in another session or in this one', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a' })
  await $.session.start(START)
  seen.tasks = [task('t030', { session: 'b', session_name: 'pane-polish' })]
  await clock.advance(3000); await clock.advance(3000)
  seen.tasks = [...seen.tasks, task('t021')]
  await clock.advance(3000); await clock.advance(3000)
  expect(seen.opened).toEqual([])
})

test('a session opened while tasks already run does not open the pane either', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: [task('t021'), task('t030', { session: 'b', session_name: 'pane-polish' })] })
  await $.session.start(START)
  await clock.advance(3000)
  expect(seen.opened).toEqual([])
})

for (const where of SURFACES) {
  test(`the row shows a task as soon as it starts, in ${where.name}`, async ($, on) => {
    const clock = mock.clock(on)
    const seen = host(on, { session: 'a' })
    await $.session.start(START)
    seen.tasks = [task('t030', { session: 'b', session_name: 'pane-polish' }), task('t021')]
    await clock.advance(3000)
    const row = await rowOf($, where)
    expect(row?.text ?? '(no row)').toContain('1 running')
  })
}

test('the pane opens when asked and stays open after every task is merged, saying there are none', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: [task('t021')] })
  await $.session.start(START)
  await $.command.run(openCommand())
  expect(seen.opened).toEqual(['delegate'])
  seen.tasks = []
  for (let i = 0; i < 4; i++) await clock.advance(3000)
  expect(seen.closed).toEqual([])
  expect(seen.open).toBe(true)
  expect(await paneText($)).toMatch(/no delegate tasks/i)
})

test('a pane the user closed stays closed while tasks start, and the row comes back', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: [task('t021')] })
  await $.session.start(START)
  await $.command.run(openCommand())
  seen.userCloses()
  seen.tasks = [task('t021'), task('t022')]
  for (let i = 0; i < 3; i++) await clock.advance(3000)
  expect(seen.opened).toEqual(['delegate'])
  const row = await rowOf($)
  expect(row?.text ?? '(no row)').toContain('2 running')
})
