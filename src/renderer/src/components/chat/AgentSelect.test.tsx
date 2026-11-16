import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { AUTO_SELECTION } from '../../modelSelection'
import { AgentSelect } from './AgentSelect'

const AGENTS = [
  { id: 'copilot', name: 'GitHub Copilot', installed: true },
  { id: 'grok', name: 'Grok CLI', installed: false, hint: 'Install Grok CLI and sign in, then pick it again.' },
  { id: 'claude', name: 'Claude Code', installed: true }
]

let setOptions: ReturnType<typeof vi.fn>

beforeEach(() => {
  setOptions = vi.fn(() => Promise.resolve(undefined))
  ;(window as unknown as { api: unknown }).api = {
    chat: {
      agents: vi.fn(() => Promise.resolve(AGENTS)),
      setOptions
    }
  }
})

afterEach(() => {
  cleanup()
  delete (window as unknown as { api?: unknown }).api
})

describe('AgentSelect', () => {
  it('lists Copilot, Grok CLI, and Claude Code, and saves the one you pick', async () => {
    const onChanged = vi.fn()
    render(<AgentSelect projectId="p1" selection={AUTO_SELECTION} onChanged={onChanged} />)
    const select = (await screen.findByRole('combobox', { name: 'Chat agent' })) as HTMLSelectElement
    expect(select.value).toBe('copilot')
    expect(await screen.findByRole('option', { name: 'Grok CLI (not installed)' })).toBeTruthy()
    fireEvent.change(select, { target: { value: 'claude' } })
    expect(select.value).toBe('claude')
    await waitFor(() => {
      expect(setOptions).toHaveBeenCalledWith('p1', expect.objectContaining({ agent: 'claude' }))
      expect(onChanged).toHaveBeenCalled()
    })
  })

  it('shows the agent already saved on the project', async () => {
    render(<AgentSelect projectId="p1" agent="claude" selection={AUTO_SELECTION} />)
    const select = (await screen.findByRole('combobox', { name: 'Chat agent' })) as HTMLSelectElement
    await screen.findByRole('option', { name: 'Claude Code' })
    expect(select.value).toBe('claude')
  })
})
