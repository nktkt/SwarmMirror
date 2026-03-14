"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback, useMemo } from "react"
import { useRouter, useParams } from "next/navigation"
import { motion } from "motion/react"
import { useAppStore } from "@/lib/store"
import { generateOntology, getProject, buildGraph, getTaskStatus, getGraphData } from "@/lib/api/graph"
import GraphPanel from "@/components/GraphPanel"
import Step1GraphBuild from "@/components/steps/Step1GraphBuild"

type ViewMode = "graph" | "split" | "workbench"

export default function ProcessPage() {
  const params = useParams()
  const router = useRouter()
  const projectId = params.projectId as string

  const { pendingFiles, pendingRequirement, isPending, clearPendingUpload } = useAppStore()

  const [viewMode, setViewMode] = useState<ViewMode>("split")
  const [currentStep] = useState(1)
  const [currentProjectId, setCurrentProjectId] = useState(projectId)
  const [projectData, setProjectData] = useState<any>(null)
  const [graphDataState, setGraphDataState] = useState<any>(null)
  const [graphLoading, setGraphLoading] = useState(false)
  const [currentPhase, setCurrentPhase] = useState(-1)
  const [ontologyProgress, setOntologyProgress] = useState<any>(null)
  const [buildProgress, setBuildProgress] = useState<any>(null)
  const [systemLogs, setSystemLogs] = useState<{ time: string; msg: string }[]>([])
  const [error, setError] = useState("")
  const [, setStatusText] = useState("Initializing")

  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const graphPollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const addLog = useCallback((msg: string) => {
    const time = new Date().toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" }) + "." + new Date().getMilliseconds().toString().padStart(3, "0")
    setSystemLogs((prev) => {
      const logs = [...prev, { time, msg }]
      return logs.length > 100 ? logs.slice(1) : logs
    })
  }, [])

  const stopPolling = useCallback(() => {
    if (pollTimerRef.current) { clearInterval(pollTimerRef.current); pollTimerRef.current = null }
  }, [])

  const stopGraphPolling = useCallback(() => {
    if (graphPollTimerRef.current) { clearInterval(graphPollTimerRef.current); graphPollTimerRef.current = null }
  }, [])

  useEffect(() => {
    return () => { stopPolling(); stopGraphPolling() }
  }, [stopPolling, stopGraphPolling])

  const loadGraph = useCallback(async (graphId: string) => {
    setGraphLoading(true)
    try {
      const res: any = await getGraphData(graphId)
      if (res.success) setGraphDataState(res.data)
    } catch (e: any) {
      addLog(`Graph load error: ${e.message}`)
    } finally {
      setGraphLoading(false)
    }
  }, [addLog])

  const fetchGraphData = useCallback(async () => {
    try {
      const projRes: any = await getProject(currentProjectId)
      if (projRes.success && projRes.data.graph_id) {
        const gRes: any = await getGraphData(projRes.data.graph_id)
        if (gRes.success) setGraphDataState(gRes.data)
      }
    } catch { /* ignore */ }
  }, [currentProjectId])

  const pollTaskStatus = useCallback(async (taskId: string) => {
    try {
      const res: any = await getTaskStatus(taskId)
      if (res.success) {
        const task = res.data
        setBuildProgress({ progress: task.progress || 0, message: task.message })
        if (task.message) addLog(task.message)
        if (task.status === "completed") {
          addLog("Graph build completed.")
          stopPolling()
          stopGraphPolling()
          setCurrentPhase(2)
          setStatusText("Ready")
          const projRes: any = await getProject(currentProjectId)
          if (projRes.success && projRes.data.graph_id) {
            setProjectData(projRes.data)
            await loadGraph(projRes.data.graph_id)
          }
        } else if (task.status === "failed") {
          stopPolling()
          setError(task.error)
          addLog(`Build failed: ${task.error}`)
        }
      }
    } catch { /* ignore */ }
  }, [currentProjectId, addLog, stopPolling, stopGraphPolling, loadGraph])

  const startBuildGraph = useCallback(async () => {
    setCurrentPhase(1)
    setBuildProgress({ progress: 0, message: "Starting build..." })
    addLog("Initiating graph build...")
    try {
      const res: any = await buildGraph({ project_id: currentProjectId })
      if (res.success) {
        addLog(`Graph build task started: ${res.data.task_id}`)
        graphPollTimerRef.current = setInterval(fetchGraphData, 10000)
        pollTimerRef.current = setInterval(() => pollTaskStatus(res.data.task_id), 2000)
      } else {
        setError(res.error)
        addLog(`Build error: ${res.error}`)
      }
    } catch (err: any) {
      setError(err.message)
      addLog(`Build exception: ${err.message}`)
    }
  }, [currentProjectId, addLog, fetchGraphData, pollTaskStatus])

  const handleNewProject = useCallback(async () => {
    if (!isPending || pendingFiles.length === 0) {
      setError("No pending files found.")
      addLog("Error: No pending files for new project.")
      return
    }
    try {
      setCurrentPhase(0)
      setOntologyProgress({ message: "Uploading and analyzing docs..." })
      addLog("Starting ontology generation...")

      const formData = new FormData()
      pendingFiles.forEach((f) => formData.append("files", f))
      formData.append("simulation_requirement", pendingRequirement)

      const res: any = await generateOntology(formData)
      if (res.success) {
        clearPendingUpload()
        setCurrentProjectId(res.data.project_id)
        setProjectData(res.data)
        router.replace(`/process/${res.data.project_id}`)
        setOntologyProgress(null)
        addLog(`Ontology generated for project ${res.data.project_id}`)
        setStatusText("Building Graph")
        await startBuildGraph()
      } else {
        setError(res.error || "Ontology generation failed")
        addLog(`Ontology error: ${res.error}`)
      }
    } catch (err: any) {
      setError(err.message)
      addLog(`Exception: ${err.message}`)
    }
  }, [isPending, pendingFiles, pendingRequirement, clearPendingUpload, router, addLog, startBuildGraph])

  const loadProject = useCallback(async () => {
    try {
      addLog(`Loading project ${currentProjectId}...`)
      const res: any = await getProject(currentProjectId)
      if (res.success) {
        setProjectData(res.data)
        const status = res.data.status
        addLog(`Project loaded. Status: ${status}`)

        if (status === "ontology_generated" && !res.data.graph_id) {
          await startBuildGraph()
        } else if (status === "graph_building" && res.data.graph_build_task_id) {
          setCurrentPhase(1)
          setStatusText("Building Graph")
          pollTimerRef.current = setInterval(() => pollTaskStatus(res.data.graph_build_task_id), 2000)
          graphPollTimerRef.current = setInterval(fetchGraphData, 10000)
        } else if (status === "graph_completed" && res.data.graph_id) {
          setCurrentPhase(2)
          setStatusText("Ready")
          await loadGraph(res.data.graph_id)
        }
      } else {
        setError(res.error)
        addLog(`Load error: ${res.error}`)
      }
    } catch (err: any) {
      setError(err.message)
      addLog(`Load exception: ${err.message}`)
    }
  }, [currentProjectId, addLog, startBuildGraph, pollTaskStatus, fetchGraphData, loadGraph])

  useEffect(() => {
    addLog("Project view initialized.")
    if (projectId === "new") {
      handleNewProject()
    } else {
      loadProject()
    }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const stepNames = ["Graph Build", "Env Setup", "Simulation", "Report", "Interaction"]

  const statusClass = useMemo(() => {
    if (error) return "bg-red-500"
    if (currentPhase >= 2) return "bg-green-500"
    return "bg-orange-500"
  }, [error, currentPhase])

  const statusLabel = useMemo(() => {
    if (error) return "Error"
    if (currentPhase >= 2) return "Ready"
    if (currentPhase === 1) return "Building Graph"
    if (currentPhase === 0) return "Generating Ontology"
    return "Initializing"
  }, [error, currentPhase])

  const leftStyle = viewMode === "graph" ? "w-full" : viewMode === "workbench" ? "w-0 opacity-0" : "w-1/2"
  const rightStyle = viewMode === "workbench" ? "w-full" : viewMode === "graph" ? "w-0 opacity-0" : "w-1/2"

  return (
    <div className="h-screen flex flex-col bg-white overflow-hidden">
      {/* Header */}
      <header className="h-[60px] border-b border-gray-200 flex items-center justify-between px-6 bg-white z-[100] relative">
        <div className="font-mono font-extrabold text-lg tracking-wider cursor-pointer" onClick={() => router.push("/")}>
          MIROFISH
        </div>
        <div className="absolute left-1/2 -translate-x-1/2">
          <div className="flex bg-gray-100 p-1 rounded-lg gap-1">
            {(["graph", "split", "workbench"] as ViewMode[]).map((mode) => (
              <button
                key={mode}
                className={`px-4 py-1.5 text-xs font-semibold rounded transition-all ${viewMode === mode ? "bg-white text-black shadow-sm" : "text-gray-500 hover:text-gray-700"}`}
                onClick={() => setViewMode(mode)}
              >
                {mode === "graph" ? "Graph" : mode === "split" ? "Split" : "Workbench"}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2 text-sm">
            <span className="font-mono font-bold text-gray-400">Step {currentStep}/5</span>
            <span className="font-bold">{stepNames[currentStep - 1]}</span>
          </div>
          <div className="w-px h-3.5 bg-gray-200" />
          <div className="flex items-center gap-2 text-xs text-gray-500 font-medium">
            <span className={`w-2 h-2 rounded-full ${statusClass} ${currentPhase < 2 && !error ? "animate-[pulse-dot_1s_infinite]" : ""}`} />
            {statusLabel}
          </div>
        </div>
      </header>

      {/* Content */}
      <main className="flex-1 flex overflow-hidden">
        <motion.div
          className={`h-full overflow-hidden transition-all duration-400 ease-[cubic-bezier(0.25,0.8,0.25,1)] ${leftStyle} border-r border-gray-200`}
        >
          <GraphPanel
            graphData={graphDataState}
            loading={graphLoading}
            currentPhase={currentPhase}
            onRefresh={() => projectData?.graph_id && loadGraph(projectData.graph_id)}
            onToggleMaximize={() => setViewMode(viewMode === "graph" ? "split" : "graph")}
          />
        </motion.div>
        <motion.div
          className={`h-full overflow-hidden transition-all duration-400 ease-[cubic-bezier(0.25,0.8,0.25,1)] ${rightStyle}`}
        >
          <Step1GraphBuild
            currentPhase={currentPhase}
            projectData={projectData}
            ontologyProgress={ontologyProgress}
            buildProgress={buildProgress}
            graphData={graphDataState}
            systemLogs={systemLogs}
            onNextStep={() => {}}
          />
        </motion.div>
      </main>
    </div>
  )
}
