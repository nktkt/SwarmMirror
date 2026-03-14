/* eslint-disable @typescript-eslint/no-explicit-any */
import service, { requestWithRetry } from './client'

export const generateReport = (data: { simulation_id: string; force_regenerate?: boolean }) => {
  return requestWithRetry(() => service.post('/api/report/generate', data), 3, 1000)
}

export const getReportStatus = (reportId: string) => {
  return service.get('/api/report/generate/status', { params: { report_id: reportId } })
}

export const getAgentLog = (reportId: string, fromLine = 0) => {
  return service.get(`/api/report/${reportId}/agent-log`, { params: { from_line: fromLine } })
}

export const getConsoleLog = (reportId: string, fromLine = 0) => {
  return service.get(`/api/report/${reportId}/console-log`, { params: { from_line: fromLine } })
}

export const getReport = (reportId: string): Promise<any> => {
  return service.get(`/api/report/${reportId}`)
}

export const chatWithReport = (data: {
  simulation_id: string
  message: string
  chat_history?: { role: string; content: string }[]
}) => {
  return requestWithRetry(() => service.post('/api/report/chat', data), 3, 1000)
}
