/* eslint-disable @typescript-eslint/no-explicit-any */
import { create } from 'zustand'

interface LogEntry {
  time: string
  msg: string
}

interface AppState {
  // Pending upload
  pendingFiles: File[]
  pendingRequirement: string
  isPending: boolean
  setPendingUpload: (files: File[], requirement: string) => void
  clearPendingUpload: () => void

  // Current project
  currentProjectId: string | null
  projectData: any
  setProjectData: (data: any) => void
  setCurrentProjectId: (id: string | null) => void

  // Graph data
  graphData: any
  setGraphData: (data: any) => void

  // Step tracking
  currentStep: number
  setCurrentStep: (step: number) => void

  // System logs
  systemLogs: LogEntry[]
  addLog: (msg: string) => void
  clearLogs: () => void

  // View mode
  viewMode: 'graph' | 'split' | 'workbench'
  setViewMode: (mode: 'graph' | 'split' | 'workbench') => void
}

export const useAppStore = create<AppState>((set) => ({
  pendingFiles: [],
  pendingRequirement: '',
  isPending: false,
  setPendingUpload: (files, requirement) =>
    set({ pendingFiles: files, pendingRequirement: requirement, isPending: true }),
  clearPendingUpload: () =>
    set({ pendingFiles: [], pendingRequirement: '', isPending: false }),

  currentProjectId: null,
  projectData: null,
  setProjectData: (data) => set({ projectData: data }),
  setCurrentProjectId: (id) => set({ currentProjectId: id }),

  graphData: null,
  setGraphData: (data) => set({ graphData: data }),

  currentStep: 1,
  setCurrentStep: (step) => set({ currentStep: step }),

  systemLogs: [],
  addLog: (msg) =>
    set((state) => {
      const time =
        new Date().toLocaleTimeString('en-US', {
          hour12: false,
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
        }) +
        '.' +
        new Date().getMilliseconds().toString().padStart(3, '0')
      const logs = [...state.systemLogs, { time, msg }]
      if (logs.length > 200) logs.shift()
      return { systemLogs: logs }
    }),
  clearLogs: () => set({ systemLogs: [] }),

  viewMode: 'split',
  setViewMode: (mode) => set({ viewMode: mode }),
}))
