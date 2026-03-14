"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback } from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import {
  prepareSimulation,
  getPrepareStatus,
  getSimulationProfiles,
  getSimulationProfilesRealtime,
  getSimulationConfig,
  getSimulationConfigRealtime,
} from "@/lib/api/simulation"

interface Step2Props {
  simulationId: string
  projectData: any
  graphData: any
  systemLogs: { time: string; msg: string }[]
  onGoBack: () => void
  onNextStep: (params?: any) => void
  onAddLog: (msg: string) => void
  onUpdateStatus?: (status: string) => void
}

export default function Step2EnvSetup({ simulationId, projectData, graphData, systemLogs, onGoBack, onNextStep, onAddLog, onUpdateStatus }: Step2Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const [phase, setPhase] = useState(0)
  const [taskId, setTaskId] = useState<string | null>(null)
  const [progress, setProgress] = useState(0)
  const [progressMsg, setProgressMsg] = useState("")
  const [profiles, setProfiles] = useState<any[]>([])
  const [config, setConfig] = useState<any>(null)
  const [maxRoundsInput, setMaxRoundsInput] = useState("")
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const profilePollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  const stopPolling = useCallback(() => {
    if (pollRef.current) { clearInterval(pollRef.current); pollRef.current = null }
    if (profilePollRef.current) { clearInterval(profilePollRef.current); profilePollRef.current = null }
  }, [])

  useEffect(() => {
    return () => stopPolling()
  }, [stopPolling])

  const startPrepare = useCallback(async () => {
    if (!simulationId) return
    onAddLog("Starting environment preparation...")
    setPhase(1)
    try {
      const res: any = await prepareSimulation({
        simulation_id: simulationId,
        use_llm_for_profiles: true,
        parallel_profile_count: 5,
      })
      if (res.success && res.data?.task_id) {
        setTaskId(res.data.task_id)
        onAddLog(`Prepare task started: ${res.data.task_id}`)
        startPolling(res.data.task_id)
        startProfilePolling()
      } else {
        onAddLog(`Prepare failed: ${res.error || "Unknown error"}`)
      }
    } catch (err: any) {
      onAddLog(`Prepare error: ${err.message}`)
    }
  }, [simulationId, onAddLog])

  useEffect(() => {
    if (simulationId && phase === 0) {
      startPrepare()
    }
  }, [simulationId, phase, startPrepare])

  const startPolling = (tid: string) => {
    const poll = async () => {
      try {
        const res: any = await getPrepareStatus({ task_id: tid })
        if (res.success && res.data) {
          const { status, progress: p, message } = res.data
          setProgress(p || 0)
          if (message) setProgressMsg(message)
          if (status === "completed") {
            setPhase(2)
            setProgress(100)
            stopPolling()
            onAddLog("Environment preparation complete")
            onUpdateStatus?.("completed")
            loadFinalData()
          } else if (status === "failed") {
            stopPolling()
            onAddLog(`Prepare failed: ${res.data.error}`)
            onUpdateStatus?.("error")
          }
        }
      } catch { /* ignore */ }
    }
    poll()
    pollRef.current = setInterval(poll, 2000)
  }

  const startProfilePolling = () => {
    const poll = async () => {
      try {
        const res: any = await getSimulationProfilesRealtime(simulationId)
        if (res.success && res.data) {
          setProfiles(res.data.profiles || [])
        }
      } catch { /* ignore */ }
      try {
        const res: any = await getSimulationConfigRealtime(simulationId)
        if (res.success && res.data) {
          setConfig(res.data)
        }
      } catch { /* ignore */ }
    }
    profilePollRef.current = setInterval(poll, 3000)
  }

  const loadFinalData = async () => {
    try {
      const [profileRes, configRes]: any[] = await Promise.all([
        getSimulationProfiles(simulationId),
        getSimulationConfig(simulationId),
      ])
      if (profileRes.success) setProfiles(profileRes.data?.profiles || [])
      if (configRes.success) setConfig(configRes.data)
    } catch { /* ignore */ }
  }

  const handleNextStep = () => {
    const maxRounds = maxRoundsInput ? parseInt(maxRoundsInput) : undefined
    router.push(`/simulation/${simulationId}/start${maxRounds ? `?maxRounds=${maxRounds}` : ""}`)
  }

  return (
    <div className="h-full flex flex-col bg-[#FAFAFA] overflow-hidden">
      <div className="flex-1 overflow-y-auto p-6 space-y-5">
        {/* Step 01: Simulation Instance */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border ${phase === 0 ? "border-primary" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${phase >= 0 ? "text-black" : "text-gray-300"}`}>01</span>
              <span className="font-semibold text-sm">Simulation Instance</span>
            </div>
            {phase > 0 ? <Badge variant="success">Complete</Badge> : <Badge variant="processing">Initializing</Badge>}
          </div>
          <div className="text-xs text-gray-400 font-mono mb-2">POST /api/simulation/create</div>
          {simulationId && (
            <div className="bg-gray-50 rounded-md p-3 space-y-2 text-xs">
              <div className="flex justify-between"><span className="text-gray-400">Project ID</span><span className="font-mono text-gray-700">{projectData?.project_id}</span></div>
              <div className="flex justify-between"><span className="text-gray-400">Graph ID</span><span className="font-mono text-gray-700">{projectData?.graph_id}</span></div>
              <div className="flex justify-between"><span className="text-gray-400">Simulation ID</span><span className="font-mono text-gray-700">{simulationId}</span></div>
              <div className="flex justify-between"><span className="text-gray-400">Task ID</span><span className="font-mono text-gray-700">{taskId || "Task completed"}</span></div>
            </div>
          )}
        </div>

        {/* Step 02: Agent Profiles */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border ${phase === 1 ? "border-primary" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${phase >= 1 ? "text-black" : "text-gray-300"}`}>02</span>
              <span className="font-semibold text-sm">Agent Profile Generation</span>
            </div>
            {phase > 1 ? <Badge variant="success">Complete</Badge>
              : phase === 1 ? <Badge variant="processing">{progress}%</Badge>
              : <Badge variant="secondary">Waiting</Badge>}
          </div>
          <div className="text-xs text-gray-400 font-mono mb-2">POST /api/simulation/prepare</div>
          <p className="text-xs text-gray-500 mb-4">Extract entities from the knowledge graph and generate detailed agent profiles using LLM.</p>

          {phase >= 1 && (
            <div className="mb-4">
              <Progress value={progress} className="h-2 mb-2" />
              <p className="text-xs text-gray-400">{progressMsg}</p>
            </div>
          )}

          {profiles.length > 0 && (
            <div className="space-y-2 max-h-[300px] overflow-y-auto">
              {profiles.slice(0, 10).map((profile: any, i: number) => (
                <div key={i} className="bg-gray-50 rounded p-3 border border-gray-100">
                  <div className="flex items-center gap-2 mb-1">
                    <div className="w-6 h-6 bg-primary text-white rounded-full flex items-center justify-center text-[10px] font-bold">
                      {(profile.name || "A")[0]}
                    </div>
                    <span className="text-xs font-semibold">{profile.name || `Agent ${i + 1}`}</span>
                    {profile.entity_type && <span className="text-[10px] text-gray-400 font-mono">{profile.entity_type}</span>}
                  </div>
                  {profile.bio && <p className="text-[11px] text-gray-500 leading-relaxed line-clamp-2">{profile.bio}</p>}
                </div>
              ))}
              {profiles.length > 10 && (
                <p className="text-xs text-gray-400 text-center font-mono">+{profiles.length - 10} more agents</p>
              )}
            </div>
          )}
        </div>

        {/* Step 03: Simulation Config */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border ${phase === 2 ? "border-primary" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${phase >= 2 ? "text-black" : "text-gray-300"}`}>03</span>
              <span className="font-semibold text-sm">Simulation Configuration</span>
            </div>
            {phase >= 2 && <Badge variant="processing">Ready</Badge>}
          </div>

          {config && (
            <div className="bg-gray-50 rounded p-3 mb-4 text-xs space-y-1.5 font-mono">
              {config.time_config && (
                <>
                  <div className="flex justify-between"><span className="text-gray-400">Minutes/Round</span><span>{config.time_config.minutes_per_round}</span></div>
                  <div className="flex justify-between"><span className="text-gray-400">Max Rounds</span><span>{config.time_config.max_rounds || "Auto"}</span></div>
                </>
              )}
              {config.platform_config && (
                <div className="flex justify-between"><span className="text-gray-400">Platforms</span><span>{Object.keys(config.platform_config).join(", ")}</span></div>
              )}
            </div>
          )}

          <div className="mb-4">
            <label className="text-xs text-gray-500 block mb-1.5">Custom Max Rounds (optional)</label>
            <input
              type="number"
              value={maxRoundsInput}
              onChange={(e) => setMaxRoundsInput(e.target.value)}
              placeholder="Auto"
              className="w-full border border-gray-200 rounded px-3 py-2 text-sm font-mono outline-none focus:border-primary"
            />
          </div>

          <div className="flex gap-3">
            <Button variant="outline" className="flex-1" onClick={onGoBack}>Back</Button>
            <Button className="flex-1" disabled={phase < 2} onClick={handleNextStep}>
              Start Simulation &rarr;
            </Button>
          </div>
        </div>
      </div>

      {/* System Logs */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span>ENVIRONMENT SETUP</span>
          <span>{simulationId || "NO_SIMULATION"}</span>
        </div>
        <div ref={logRef} className="flex flex-col gap-1 h-20 overflow-y-auto pr-1">
          {systemLogs.map((log, i) => (
            <div key={i} className="text-[11px] flex gap-3 leading-relaxed">
              <span className="text-gray-600 min-w-[75px]">{log.time}</span>
              <span className="text-gray-400 break-all">{log.msg}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
