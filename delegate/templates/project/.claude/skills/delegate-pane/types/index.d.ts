/** One worker step, as `delegate status --json` gives it. */
export type Step = { kind: string; target?: string | null; result?: string | null }

/** One task, the fields of `delegate status --json` the pane draws. */
export type Task = {
  id: string
  title: string
  goal: string
  session: string | null
  session_name: string
  phase: string
  outcome: string | null
  worker: string | null
  started_at: string | null
  last_activity_at: string | null
  stalled: boolean
  changed_files: { path: string }[]
  steps: Step[]
  ready_to_merge: boolean
  merge_blockers: string[]
  next: string | null
}

/** The autopilot lead, as `delegate status --json` gives it; absent when autopilot never ran. */
export type Autopilot = {
  running: boolean
  wave: number
  max_waves: number
  step: Step | null
  /** The lead's last five steps, oldest first. */
  steps: Step[]
  stop_reason: string | null
  /** Tasks parked under "Waiting for you". */
  waiting: number
}

/** What the pane last read: the tasks and the autopilot lead, or why there are none. */
export type View = { tasks: Task[]; error: string | null; readAt: number; autopilot: Autopilot | null }

declare module 'claude-code' {
  interface PluginState {
    'delegate-pane': { view: View }
  }
}
