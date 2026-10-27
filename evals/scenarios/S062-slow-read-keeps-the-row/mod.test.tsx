import { expect, test } from 'claude-code/testing'
import { host, mock, openCommand, paneText, rowOf, START, task } from './mod-host'

// A session with two running tasks; then every `delegate status` takes longer than a minute.
async function slowSession($: any, on: any) {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: [task('t001'), task('t002')] })
  await $.session.start(START)
  await clock.advance(3000)
  seen.slow = true
  return { clock, seen }
}

test('with delegate slow, the session never runs more than one delegate status at a time', async ($, on) => {
  const { clock, seen } = await slowSession($, on)
  for (let i = 0; i < 20; i++) await clock.advance(3000)
  expect(seen.maxInflight).toBe(1)
})

test('while a read is slow, the row keeps the last result and never says status unavailable', async ($, on) => {
  const { clock, seen } = await slowSession($, on)
  seen.userCloses()
  for (let i = 0; i < 10; i++) await clock.advance(3000)
  const r = await rowOf($)
  expect(r?.text ?? '(no row)').toContain('2 running')
  expect(r?.text ?? '').not.toContain('unavailable')
})

test('while a read is slow, /delegate-pane opens the pane at once, showing the tasks it last read', async ($, on) => {
  const { clock } = await slowSession($, on)
  const opened = await Promise.race([
    $.command.run(openCommand()).then(() => true),
    new Promise<boolean>(r => setTimeout(() => r(false), 1000)),
  ])
  expect(opened).toBe(true)
  for (let i = 0; i < 10; i++) await clock.advance(3000)
  const text = await paneText($)
  expect(text).toMatch(/t001/)
  expect(text).toMatch(/t002/)
  expect(text).not.toMatch(/unavailable|failed/i)
})

test('once a slow read answers, the row shows it within the next refresh', async ($, on) => {
  const { clock, seen } = await slowSession($, on)
  seen.userCloses()
  for (let i = 0; i < 5; i++) await clock.advance(3000)
  seen.tasks = [task('t001', { phase: 'done', outcome: 'pass', ready_to_merge: true }), task('t002')]
  seen.slow = false
  seen.release()
  await clock.advance(3000); await clock.advance(3000)
  const r = await rowOf($)
  expect(r?.text ?? '(no row)').toContain('1 ready to merge')
  expect(r?.text ?? '').toContain('1 running')
})

test('a read that fails says status unavailable, and the next good read replaces it', async ($, on) => {
  const clock = mock.clock(on)
  const seen = host(on, { session: 'a', tasks: [task('t001')] })
  await $.session.start(START)
  seen.userCloses()
  seen.fail = true
  await clock.advance(3000)
  expect((await rowOf($))?.text ?? '(no row)').toContain('delegate: status unavailable')
  seen.fail = false
  await clock.advance(3000)
  const r = await rowOf($)
  expect(r?.text ?? '(no row)').toContain('1 running')
})
