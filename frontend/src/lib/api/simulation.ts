/* eslint-disable @typescript-eslint/no-explicit-any */
import service, { requestWithRetry } from './client'

export const createSimulation = (data: {
  project_id: string
  graph_id?: string
  enable_twitter?: boolean
  enable_reddit?: boolean
}) => {
  return requestWithRetry(() => service.post('/api/simulation/create', data), 3, 1000)
}

export const prepareSimulation = (data: {
  simulation_id: string
  entity_types?: string[]
  use_llm_for_profiles?: boolean
  parallel_profile_count?: number
  force_regenerate?: boolean
}) => {
  return requestWithRetry(() => service.post('/api/simulation/prepare', data), 3, 1000)
}

export const getPrepareStatus = (data: { task_id?: string; simulation_id?: string }) => {
  return service.post('/api/simulation/prepare/status', data)
}

export const getSimulation = (simulationId: string): Promise<any> => {
  return service.get(`/api/simulation/${simulationId}`)
}

export const getSimulationProfiles = (simulationId: string, platform = 'reddit') => {
  return service.get(`/api/simulation/${simulationId}/profiles`, { params: { platform } })
}

export const getSimulationProfilesRealtime = (simulationId: string, platform = 'reddit') => {
  return service.get(`/api/simulation/${simulationId}/profiles/realtime`, { params: { platform } })
}

export const getSimulationConfig = (simulationId: string) => {
  return service.get(`/api/simulation/${simulationId}/config`)
}

export const getSimulationConfigRealtime = (simulationId: string) => {
  return service.get(`/api/simulation/${simulationId}/config/realtime`)
}

export const listSimulations = (projectId?: string) => {
  const params = projectId ? { project_id: projectId } : {}
  return service.get('/api/simulation/list', { params })
}

export const startSimulation = (data: {
  simulation_id: string
  platform?: string
  max_rounds?: number
  enable_graph_memory_update?: boolean
  force?: boolean
}) => {
  return requestWithRetry(() => service.post('/api/simulation/start', data), 3, 1000)
}

export const stopSimulation = (data: { simulation_id: string }) => {
  return service.post('/api/simulation/stop', data)
}

export const getRunStatus = (simulationId: string) => {
  return service.get(`/api/simulation/${simulationId}/run-status`)
}

export const getRunStatusDetail = (simulationId: string) => {
  return service.get(`/api/simulation/${simulationId}/run-status/detail`)
}

export const getSimulationPosts = (
  simulationId: string,
  platform = 'reddit',
  limit = 50,
  offset = 0
) => {
  return service.get(`/api/simulation/${simulationId}/posts`, {
    params: { platform, limit, offset }
  })
}

export const getSimulationTimeline = (
  simulationId: string,
  startRound = 0,
  endRound: number | null = null
) => {
  const params: Record<string, number> = { start_round: startRound }
  if (endRound !== null) params.end_round = endRound
  return service.get(`/api/simulation/${simulationId}/timeline`, { params })
}

export const getAgentStats = (simulationId: string) => {
  return service.get(`/api/simulation/${simulationId}/agent-stats`)
}

export const getSimulationActions = (
  simulationId: string,
  params: Record<string, unknown> = {}
) => {
  return service.get(`/api/simulation/${simulationId}/actions`, { params })
}

export const closeSimulationEnv = (data: { simulation_id: string; timeout?: number }) => {
  return service.post('/api/simulation/close-env', data)
}

export const getEnvStatus = (data: { simulation_id: string }) => {
  return service.post('/api/simulation/env-status', data)
}

export const interviewAgents = (data: {
  simulation_id: string
  interviews: { agent_id: string; prompt: string }[]
}) => {
  return requestWithRetry(() => service.post('/api/simulation/interview/batch', data), 3, 1000)
}

export const getSimulationHistory = (limit = 20) => {
  return service.get('/api/simulation/history', { params: { limit } })
}
