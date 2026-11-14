import { currentWindowLabel } from '../bridge'

/** The stand-in for the desktop window handle. The label comes from the URL. */
export function getCurrentWindow(): { label: string; setTitle: (title: string) => Promise<void> } {
  return {
    label: currentWindowLabel(),
    setTitle: async (title: string) => {
      document.title = title
    }
  }
}
