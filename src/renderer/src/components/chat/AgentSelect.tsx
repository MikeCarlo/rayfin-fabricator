import { useEffect, useState, type ChangeEvent, type JSX } from 'react'
import type { AgentOption } from '@shared/ipc'
import { selectionArgs, type ModelSelection } from '../../modelSelection'

const COPILOT: AgentOption = { id: 'copilot', name: 'GitHub Copilot', installed: true }

/**
 * The chat's authoring agent, to the left of the model picker. Copilot stays
 * the default. Grok CLI and Claude Code are listed even when they are not
 * installed; the hint says so, and the next turn reports that the CLI is missing.
 */
export function AgentSelect({
  projectId,
  agent,
  selection,
  disabled = false,
  onChanged
}: {
  projectId: string
  /** Saved on the project. Absent is Copilot. */
  agent?: string
  selection: ModelSelection
  disabled?: boolean
  onChanged?: () => void
}): JSX.Element {
  const [options, setOptions] = useState<AgentOption[]>([COPILOT])
  const [value, setValue] = useState(agent?.trim() || COPILOT.id)
  const selected = options.find((item) => item.id === value)

  useEffect(() => {
    setValue(agent?.trim() || COPILOT.id)
  }, [agent])

  useEffect(() => {
    let cancel = false
    window.api.chat.agents().then(
      (next) => {
        if (!cancel && next.length > 0) setOptions(next)
      },
      () => {
        // Copilot remains the only choice. Failing to list CLIs is the same as none being installed.
      }
    )
    return () => {
      cancel = true
    }
  }, [projectId])

  function onChange(event: ChangeEvent<HTMLSelectElement>): void {
    const next = event.target.value
    const previous = value
    setValue(next)
    void window.api.chat.setOptions(projectId, { ...selectionArgs(selection), agent: next }).then(
      () => onChanged?.(),
      () => setValue(previous)
    )
  }

  return (
    <select
      className="agent-select"
      aria-label="Chat agent"
      title={selected?.hint ?? 'Which agent writes this project'}
      value={options.some((item) => item.id === value) ? value : COPILOT.id}
      disabled={disabled}
      onChange={onChange}
    >
      {options.map((item) => (
        <option key={item.id} value={item.id}>
          {item.installed ? item.name : `${item.name} (not installed)`}
        </option>
      ))}
    </select>
  )
}
