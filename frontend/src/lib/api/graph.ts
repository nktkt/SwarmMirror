/* eslint-disable @typescript-eslint/no-explicit-any */
import service, { requestWithRetry } from './client'

export function generateOntology(formData: FormData) {
  return requestWithRetry(() =>
    service({
      url: '/api/graph/ontology/generate',
      method: 'post',
      data: formData,
      headers: { 'Content-Type': 'multipart/form-data' }
    })
  )
}

export function buildGraph(data: { project_id: string; graph_name?: string }) {
  return requestWithRetry(() =>
    service({ url: '/api/graph/build', method: 'post', data })
  )
}

export function getTaskStatus(taskId: string) {
  return service({ url: `/api/graph/task/${taskId}`, method: 'get' })
}

export function getGraphData(graphId: string) {
  return service({ url: `/api/graph/data/${graphId}`, method: 'get' })
}

export function getProject(projectId: string): Promise<any> {
  return service({ url: `/api/graph/project/${projectId}`, method: 'get' })
}
