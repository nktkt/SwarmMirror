"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useCallback } from "react"
import { useRouter, useParams, useSearchParams } from "next/navigation"
import GraphPanel from "@/components/GraphPanel"
import Step3Simulation from "@/components/steps/Step3Simulation"
import { getProject, getGraphData } from "@/lib/api/graph"
import { getSimulation, getSimulationConfig } from "@/lib/api/simulation"

type ViewMode = "graph" | "split" | "workbench"

export default function SimulationStartPage() {
  const params = useParams()
  const searchParams = useSearchParams()
  const router = useRouter()
  const simulationId = params.simulationId as string
  const maxRoundsParam = searchParams.get("maxRounds")
  const maxRounds = maxRoundsParam ? parseInt(maxRoundsParam) : null

  const [viewMode, setViewMode] = useState<ViewMode>("split")
  const [projectData, setProjectData] = useState<any>(null)
  const [graphData, setGraphData] = useState<any>(null)
  const [graphLoading, setGraphLoading] = useState(false)
  const [minutesPerRound, setMinutesPerRound] = useState(30)
  const [systemLogs, setSystemLogs] = useState<{ time: string; msg: string }[]>([])
  const [currentStatus, setCurrentStatus] = useState("processing")

  const addLog = useCallback((msg: string) => {
    const time = new Date().toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" }) + "." + new Date().getMilliseconds().toString().padStart(3, "0")
    setSystemLogs((prev) => {
      const logs = [...prev, { time, msg }]
      return logs.length > 200 ? logs.slice(1) : logs
    })
  }, [])

  const loadGraph = useCallback(async (graphId: string) => {
    setGraphLoading(true)
    try {
      const res: any = await getGraphData(graphId)
      if (res.success) setGraphData(res.data)
    } catch { /* ignore */ }
    finally { setGraphLoading(false) }
  }, [])

  const loadSimulationData = useCallback(async () => {
    try {
      addLog(`Loading simulation: ${simulationId}`)
      const simRes: any = await getSimulation(simulationId)
      if (simRes.success && simRes.data) {
        try {
          const configRes: any = await getSimulationConfig(simulationId)
          if (configRes.success && configRes.data?.time_config?.minutes_per_round) {
            setMinutesPerRound(configRes.data.time_config.minutes_per_round)
          }
        } catch { /* ignore */ }
        if (simRes.data.project_id) {
          const projRes: any = await getProject(simRes.data.project_id)
          if (projRes.success) {
            setProjectData(projRes.data)
            if (projRes.data.graph_id) await loadGraph(projRes.data.graph_id)
          }
        }
      }
    } catch (err: any) { addLog(`Load error: ${err.message}`) }
  }, [simulationId, addLog, loadGraph])

  useEffect(() => {
    addLog("SimulationRunView initialized")
    if (maxRounds) addLog(`Custom max rounds: ${maxRounds}`)
    loadSimulationData()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const isSimulating = currentStatus === "processing"
  const statusClass = currentStatus === "error" ? "bg-red-500" : currentStatus === "completed" ? "bg-green-500" : "bg-orange-500 animate-[pulse-dot_1s_infinite]"
  const statusLabel = currentStatus === "error" ? "Error" : currentStatus === "completed" ? "Completed" : "Running"

  const leftStyle = viewMode === "graph" ? "w-full" : viewMode === "workbench" ? "w-0 opacity-0" : "w-1/2"
  const rightStyle = viewMode === "workbench" ? "w-full" : viewMode === "graph" ? "w-0 opacity-0" : "w-1/2"

  return (
    <div className="h-screen flex flex-col bg-white overflow-hidden">
      <header className="h-[60px] border-b border-gray-200 flex items-center justify-between px-6 bg-white z-[100] relative">
        <div className="font-mono font-extrabold text-lg tracking-wider cursor-pointer" onClick={() => router.push("/")}>MIROFISH</div>
        <div className="absolute left-1/2 -translate-x-1/2">
          <div className="flex bg-gray-100 p-1 rounded-lg gap-1">
            {(["graph", "split", "workbench"] as ViewMode[]).map((mode) => (
              <button key={mode} className={`px-4 py-1.5 text-xs font-semibold rounded transition-all ${viewMode === mode ? "bg-white text-black shadow-sm" : "text-gray-500"}`} onClick={() => setViewMode(mode)}>
                {mode === "graph" ? "Graph" : mode === "split" ? "Split" : "Workbench"}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-4">
          <div className="flex items-center gap-2 text-sm"><span className="font-mono font-bold text-gray-400">Step 3/5</span><span className="font-bold">Simulation</span></div>
          <div className="w-px h-3.5 bg-gray-200" />
          <div className="flex items-center gap-2 text-xs text-gray-500 font-medium"><span className={`w-2 h-2 rounded-full ${statusClass}`} />{statusLabel}</div>
        </div>
      </header>

      <main className="flex-1 flex overflow-hidden">
        <div className={`h-full overflow-hidden transition-all duration-400 ease-[cubic-bezier(0.25,0.8,0.25,1)] ${leftStyle} border-r border-gray-200`}>
          <GraphPanel graphData={graphData} loading={graphLoading} currentPhase={3} isSimulating={isSimulating} onRefresh={() => projectData?.graph_id && loadGraph(projectData.graph_id)} onToggleMaximize={() => setViewMode(viewMode === "graph" ? "split" : "graph")} />
        </div>
        <div className={`h-full overflow-hidden transition-all duration-400 ease-[cubic-bezier(0.25,0.8,0.25,1)] ${rightStyle}`}>
          <Step3Simulation
            simulationId={simulationId}
            maxRounds={maxRounds}
            minutesPerRound={minutesPerRound}
            projectData={projectData}
            graphData={graphData}
            systemLogs={systemLogs}
            onGoBack={() => router.push(`/simulation/${simulationId}`)}
            onNextStep={() => {}}
            onAddLog={addLog}
            onUpdateStatus={setCurrentStatus}
          />
        </div>
      </main>
    </div>
  )
}
