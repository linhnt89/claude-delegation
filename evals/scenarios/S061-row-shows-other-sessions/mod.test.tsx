import { expect, test } from 'claude-code/testing'
import { host, mock, rowOf, START, SURFACES } from './mod-host'
import { A, B, SESS_A, SESS_B } from './scene'

// The row of a session that reads `view`, after one refresh, with the pane closed.
async function row($: any, on: any, session: string | null, view: { tasks: any[] }, where: any = SURFACES[0], columns = 200) {
  const clock = mock.clock(on)
  const seen = host(on, { session, tasks: view.tasks })
  await $.session.start(START)
  await clock.advance(3000)
  seen.userCloses()
  return rowOf($, where, columns)
}

// Pieces of the row after the divider (the other groups) are all dimmed.
function othersDimmed(r: any): boolean {
  const i = r.parts.findIndex((p: any) => p.text.includes('│'))
  return i >= 0 && r.parts.slice(i).every((p: any) => p.props.dimColor === true || !p.text.trim())
}

for (const where of SURFACES) {
  test(`session A's row shows its counts, then pane-polish 2 (1 stalled) and by hand 1 dimmed, in ${where.name}`, async ($, on) => {
    const r = await row($, on, SESS_A, A, where)
    const text = r?.text ?? '(no row)'
    const [mine, others = ''] = text.split('│')
    expect(mine).toContain('1 running')
    expect(mine).toContain('1 ready to merge')
    expect(mine).not.toContain('stalled')
    expect(others).toMatch(/pane-polish[^·]*2[^·]*1 stalled/)
    expect(others).toContain('by hand 1')
    expect(othersDimmed(r)).toBe(true)
  })
}

test(`session B's row shows its own counts, then shop 2 and by hand 1 dimmed`, async ($, on) => {
  const r = await row($, on, SESS_B, B)
  const text = r?.text ?? '(no row)'
  const [mine, others = ''] = text.split('│')
  expect(mine).toContain('1 stalled')
  expect(mine).toContain('1 running')
  expect(others).toMatch(/(shop|main)[^·]* 2/)
  expect(others).toContain('by hand 1')
  expect(othersDimmed(r)).toBe(true)
})

test('a session with no tasks of its own still shows the row, with only the dimmed groups', async ($, on) => {
  const r = await row($, on, '8d000000-0000-4000-8000-00000000000c', A)
  const text = r?.text ?? '(no row)'
  expect(text).toMatch(/pane-polish[^·]*2/)
  expect(text).toMatch(/(shop|main)[^·]* 2/)
  expect(text).toContain('by hand 1')
  expect(r?.parts.some((p: any) => p.props.color)).toBe(false)
})

test('a narrow row cuts the other groups before this session\'s counts', async ($, on) => {
  const r = await row($, on, SESS_A, A, SURFACES[0], 58)
  const text = r?.text ?? '(no row)'
  expect(text).toContain('1 ready to merge')
  expect(text).toContain('1 running')
  expect(text).not.toContain('by hand 1')
})

test('with no session id every task counts as this session\'s, as before', async ($, on) => {
  const r = await row($, on, null, A)
  const text = r?.text ?? '(no row)'
  expect(text).toContain('1 stalled')
  expect(text).toContain('3 running')
  expect(text).toContain('1 ready to merge')
  expect(text).not.toContain('│')
})
