import { expect, test } from 'claude-code/testing'
import { host, mock, PANE, START, task } from './mod-host'

const LONG = 'Round prices per line item instead of per invoice for multi-currency carts'
const TASKS = [
  task('t001', { title: LONG, goal: 'Goal one', worker: 'pi:gpt-x@high' }),
  task('t002', { title: LONG, goal: 'Goal two', worker: 'codex:gpt-5@medium', stalled: true }),
  task('t003', { title: LONG, goal: 'Goal three', worker: 'claude:sonnet@low', phase: 'done', outcome: 'pass', ready_to_merge: true }),
  task('t004', { title: LONG, goal: 'Goal four', worker: 'gemini:flash', phase: 'not_started' }),
]

async function pane($: any, on: any, tasks = TASKS, columns = 40) {
  const clock = mock.clock(on)
  host(on, { session: 'a', tasks })
  await $.session.start(START)
  await clock.advance(3000)
  const ui = await $.ui.mount({ ...PANE, surface: 'desktop', props: { ...PANE.props, bodyColumns: columns } })
  const texts = await ui.findAll({ type: 'Text' })
  await ui.unmount()
  return texts
}

for (const t of TASKS) {
  test(`the ${t.id} card (${t.phase}${t.stalled ? ', stalled' : ''}) shows ${t.worker} in full on its own dim line under the title`, async ($, on) => {
    const texts = await pane($, on)
    const id = texts.findIndex((x: any) => x.text.startsWith(`◆ ${t.id} `))
    const worker = texts.findIndex((x: any) => x.text.trim() === t.worker)
    const goal = texts.findIndex((x: any) => x.text === t.goal)
    expect(id).toBeGreaterThanOrEqual(0)
    expect(worker).toBeGreaterThan(id)
    expect(worker).toBeLessThan(goal)
    expect(texts[worker].props.dimColor).toBe(true)
    expect(texts[id].text).not.toContain(t.worker)
  })
}

test('a task with no worker recorded has no empty worker line', async ($, on) => {
  const texts = await pane($, on, [task('t005', { title: 'Short', goal: 'Goal five', worker: null })])
  const id = texts.findIndex((x: any) => x.text.startsWith('◆ t005 '))
  const goal = texts.findIndex((x: any) => x.text === 'Goal five')
  const pieces = ['◆ ', 't005', ' Short', ' '] // the identity line's parts and the card's empty left bar
  expect(texts.slice(id + 1, goal).map((x: any) => x.text).filter((x: string) => !pieces.includes(x))).toEqual([])
})
