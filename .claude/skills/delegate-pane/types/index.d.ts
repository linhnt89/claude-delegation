/** One worker step, as `delegate status --json` gives it. */
export type Step = { kind: string; target?: string | null; result?: string | null }

/** One task, the fields of `delegate status --json` the pane draws. */
export type Task = {
  id: string
  phase: string
  outcome: string | null
  worker: string | null
  started_at: string | null
  last_activity_at: string | null
  stalled: boolean
  changed_files: { path: string }[]
  steps: Step[]
  ready_to_merge: boolean
  next: string | null
}

/** What the pane last read: the tasks, or why there are none. */
export type View = { tasks: Task[]; error: string | null; readAt: number }

declare module 'claude-code' {
  interface PluginState {
    'delegate-pane': { view: View }
  }
}
