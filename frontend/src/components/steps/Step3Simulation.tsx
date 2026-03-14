"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback } from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { startSimulation, getRunStatus, getRunStatusDetail } from "@/lib/api/simulation"
import { generateReport } from "@/lib/api/report"

interface Step3Props {
  simulationId: string
  maxRounds: number | null
  minutesPerRound: number
  projectData: any
  graphData: any
  systemLogs: { time: string; msg: string }[]
  onGoBack: () => void
  onNextStep: () => void
  onAddLog: (msg: string) => void
  onUpdateStatus: (status: string) => void
}

function getActionTypeLabel(type: string) {
  const labels: Record<string, string> = {
    CREATE_POST: "POST", REPOST: "REPOST", LIKE_POST: "LIKE",
    CREATE_COMMENT: "COMMENT", DO_NOTHING: "IDLE", FOLLOW: "FOLLOW",
    SEARCH_POSTS: "SEARCH", QUOTE_POST: "QUOTE", UPVOTE_POST: "UPVOTE", DOWNVOTE_POST: "DOWNVOTE",
  }
  return labels[type] || type || "UNKNOWN"
}

function formatActionTime(ts: string) {
  if (!ts) return ""
  try { return new Date(ts).toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" }) } catch { return "" }
}

function truncateContent(content: string, max = 100) {
  if (!content) return ""
  return content.length > max ? content.substring(0, max) + "..." : content
}

export default function Step3Simulation({ simulationId, maxRounds, minutesPerRound, systemLogs, onAddLog, onUpdateStatus }: Step3Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const [phase, setPhase] = useState(0)
  const [runStatus, setRunStatus] = useState<any>({})
  const [allActions, setAllActions] = useState<any[]>([])
  const [actionIds] = useState(() => new Set<string>())
  const [isGeneratingReport, setIsGeneratingReport] = useState(false)
  const statusTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const detailTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const prevTwitterRound = useRef(0)
  const prevRedditRound = useRef(0)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  const stopPolling = useCallback(() => {
    if (statusTimerRef.current) { clearInterval(statusTimerRef.current); statusTimerRef.current = null }
    if (detailTimerRef.current) { clearInterval(detailTimerRef.current); detailTimerRef.current = null }
  }, [])

  useEffect(() => { return () => stopPolling() }, [stopPolling])

  const formatElapsedTime = (round: number) => {
    if (!round || round <= 0) return "0h 0m"
    const total = round * minutesPerRound
    return `${Math.floor(total / 60)}h ${total % 60}m`
  }

  const doStartSimulation = useCallback(async () => {
    if (!simulationId) return
    onAddLog("Starting dual-platform parallel simulation...")
    onUpdateStatus("processing")

    try {
      const params: any = {
        simulation_id: simulationId,
        platform: "parallel",
        force: true,
        enable_graph_memory_update: true,
      }
      if (maxRounds) params.max_rounds = maxRounds

      const res: any = await startSimulation(params)
      if (res.success && res.data) {
        onAddLog("Simulation engine started successfully")
        setPhase(1)
        setRunStatus(res.data)
        // Start polling
        statusTimerRef.current = setInterval(fetchRunStatus, 2000)
        detailTimerRef.current = setInterval(fetchRunStatusDetail, 3000)
      } else {
        onAddLog(`Start failed: ${res.error || "Unknown error"}`)
        onUpdateStatus("error")
      }
    } catch (err: any) {
      onAddLog(`Start error: ${err.message}`)
      onUpdateStatus("error")
    }
  }, [simulationId, maxRounds, onAddLog, onUpdateStatus])

  useEffect(() => {
    if (simulationId) doStartSimulation()
  }, [simulationId, doStartSimulation])

  const fetchRunStatus = async () => {
    if (!simulationId) return
    try {
      const res: any = await getRunStatus(simulationId)
      if (res.success && res.data) {
        const data = res.data
        setRunStatus(data)
        if (data.twitter_current_round > prevTwitterRound.current) {
          onAddLog(`[Plaza] R${data.twitter_current_round}/${data.total_rounds} | A:${data.twitter_actions_count}`)
          prevTwitterRound.current = data.twitter_current_round
        }
        if (data.reddit_current_round > prevRedditRound.current) {
          onAddLog(`[Community] R${data.reddit_current_round}/${data.total_rounds} | A:${data.reddit_actions_count}`)
          prevRedditRound.current = data.reddit_current_round
        }
        const isCompleted = data.runner_status === "completed" || data.runner_status === "stopped"
        const platformsDone = data.twitter_completed && data.reddit_completed
        if (isCompleted || platformsDone) {
          onAddLog("Simulation completed")
          setPhase(2)
          stopPolling()
          onUpdateStatus("completed")
        }
      }
    } catch { /* ignore */ }
  }

  const fetchRunStatusDetail = async () => {
    if (!simulationId) return
    try {
      const res: any = await getRunStatusDetail(simulationId)
      if (res.success && res.data) {
        const serverActions = res.data.all_actions || []
        serverActions.forEach((action: any) => {
          const id = action.id || `${action.timestamp}-${action.platform}-${action.agent_id}-${action.action_type}`
          if (!actionIds.has(id)) {
            actionIds.add(id)
            setAllActions((prev) => [...prev, { ...action, _uniqueId: id }])
          }
        })
      }
    } catch { /* ignore */ }
  }

  const handleNextStep = async () => {
    if (!simulationId || isGeneratingReport) return
    setIsGeneratingReport(true)
    onAddLog("Starting report generation...")
    try {
      const res: any = await generateReport({ simulation_id: simulationId, force_regenerate: true })
      if (res.success && res.data) {
        onAddLog(`Report generation started: ${res.data.report_id}`)
        router.push(`/report/${res.data.report_id}`)
      } else {
        onAddLog(`Report generation failed: ${res.error || "Unknown"}`)
        setIsGeneratingReport(false)
      }
    } catch (err: any) {
      onAddLog(`Report generation error: ${err.message}`)
      setIsGeneratingReport(false)
    }
  }

  const twitterCount = allActions.filter((a) => a.platform === "twitter").length
  const redditCount = allActions.filter((a) => a.platform === "reddit").length

  return (
    <div className="h-full flex flex-col bg-white overflow-hidden">
      {/* Control Bar */}
      <div className="bg-white px-6 py-3 flex justify-between items-center border-b border-gray-200 flex-shrink-0">
        <div className="flex gap-3">
          {/* Twitter Platform */}
          <div className={`flex flex-col gap-1 px-3 py-1.5 rounded border text-xs min-w-[140px] ${runStatus.twitter_running ? "border-gray-800 bg-white" : runStatus.twitter_completed ? "border-green-600 bg-green-50" : "border-gray-200 bg-gray-50 opacity-70"}`}>
            <div className="flex items-center gap-2 font-bold text-[11px] uppercase tracking-wide">Info Plaza {runStatus.twitter_completed && <span className="text-green-600">&#10003;</span>}</div>
            <div className="flex gap-2.5 text-gray-500">
              <span>R<span className="font-mono font-semibold text-gray-800">{runStatus.twitter_current_round || 0}</span>/{runStatus.total_rounds || maxRounds || "-"}</span>
              <span>{formatElapsedTime(runStatus.twitter_current_round || 0)}</span>
              <span>A:{runStatus.twitter_actions_count || 0}</span>
            </div>
          </div>
          {/* Reddit Platform */}
          <div className={`flex flex-col gap-1 px-3 py-1.5 rounded border text-xs min-w-[140px] ${runStatus.reddit_running ? "border-gray-800 bg-white" : runStatus.reddit_completed ? "border-green-600 bg-green-50" : "border-gray-200 bg-gray-50 opacity-70"}`}>
            <div className="flex items-center gap-2 font-bold text-[11px] uppercase tracking-wide">Topic Community {runStatus.reddit_completed && <span className="text-green-600">&#10003;</span>}</div>
            <div className="flex gap-2.5 text-gray-500">
              <span>R<span className="font-mono font-semibold text-gray-800">{runStatus.reddit_current_round || 0}</span>/{runStatus.total_rounds || maxRounds || "-"}</span>
              <span>{formatElapsedTime(runStatus.reddit_current_round || 0)}</span>
              <span>A:{runStatus.reddit_actions_count || 0}</span>
            </div>
          </div>
        </div>
        <Button disabled={phase !== 2 || isGeneratingReport} onClick={handleNextStep} className="uppercase tracking-wide text-xs">
          {isGeneratingReport ? "Starting..." : "Generate Report"} &rarr;
        </Button>
      </div>

      {/* Timeline Feed */}
      <div className="flex-1 overflow-y-auto relative bg-white">
        {allActions.length > 0 && (
          <div className="sticky top-0 bg-white/90 backdrop-blur-sm py-3 px-6 border-b border-gray-200 z-10 flex justify-center">
            <div className="flex items-center gap-4 text-[11px] text-gray-500 bg-gray-100 px-3 py-1 rounded-full">
              <span className="font-semibold text-gray-800">TOTAL: <span className="font-mono">{allActions.length}</span></span>
              <span className="font-mono">{twitterCount}</span> / <span className="font-mono">{redditCount}</span>
            </div>
          </div>
        )}

        <div className="py-6 relative max-w-[900px] mx-auto min-h-full">
          <div className="absolute left-1/2 top-0 bottom-0 w-px bg-gray-200 -translate-x-1/2" />

          {allActions.map((action) => (
            <div
              key={action._uniqueId}
              className={`flex justify-center mb-8 relative w-full ${action.platform === "twitter" ? "justify-start pr-[50%]" : "justify-end pl-[50%]"}`}
            >
              <div className="absolute left-1/2 top-6 w-2.5 h-2.5 bg-white border border-gray-400 rounded-full -translate-x-1/2 z-[2]">
                <div className="w-1 h-1 bg-gray-800 rounded-full m-auto mt-[3px]" />
              </div>

              <div className={`w-[calc(100%-48px)] bg-white rounded border border-gray-200 p-4 hover:shadow-md transition-shadow ${action.platform === "twitter" ? "ml-auto mr-8" : "mr-auto ml-8"}`}>
                <div className="flex justify-between items-start mb-3 pb-3 border-b border-gray-100">
                  <div className="flex items-center gap-2.5">
                    <div className="w-6 h-6 bg-black text-white rounded-full flex items-center justify-center text-[11px] font-bold uppercase">
                      {(action.agent_name || "A")[0]}
                    </div>
                    <span className="text-sm font-semibold">{action.agent_name}</span>
                  </div>
                  <Badge variant="outline" className="text-[9px] uppercase tracking-wide">{getActionTypeLabel(action.action_type)}</Badge>
                </div>

                <div className="text-sm text-gray-700 leading-relaxed">
                  {action.action_type === "CREATE_POST" && action.action_args?.content && (
                    <p className="text-gray-900">{action.action_args.content}</p>
                  )}
                  {action.action_type === "QUOTE_POST" && (
                    <>
                      {action.action_args?.quote_content && <p>{action.action_args.quote_content}</p>}
                      {action.action_args?.original_content && (
                        <div className="bg-gray-50 border border-gray-100 p-2.5 rounded mt-2 text-xs text-gray-500">
                          @{action.action_args.original_author_name || "User"}: {truncateContent(action.action_args.original_content, 150)}
                        </div>
                      )}
                    </>
                  )}
                  {action.action_type === "LIKE_POST" && (
                    <p className="text-xs text-gray-500">Liked @{action.action_args?.post_author_name || "User"}&apos;s post</p>
                  )}
                  {action.action_type === "CREATE_COMMENT" && action.action_args?.content && (
                    <p>{action.action_args.content}</p>
                  )}
                  {action.action_type === "DO_NOTHING" && (
                    <p className="text-xs text-gray-400">Action skipped</p>
                  )}
                  {action.action_type === "FOLLOW" && (
                    <p className="text-xs text-gray-500">Followed @{action.action_args?.target_user || "User"}</p>
                  )}
                </div>

                <div className="mt-3 flex justify-end text-[10px] text-gray-300 font-mono">
                  R{action.round_num} &bull; {formatActionTime(action.timestamp)}
                </div>
              </div>
            </div>
          ))}

          {allActions.length === 0 && (
            <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col items-center gap-4 text-gray-300 text-xs uppercase tracking-widest">
              <div className="w-8 h-8 rounded-full border border-gray-200 animate-[ripple_2s_infinite]" />
              <span>Waiting for agent actions...</span>
            </div>
          )}
        </div>
      </div>

      {/* System Logs */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span>SIMULATION MONITOR</span>
          <span>{simulationId || "NO_SIMULATION"}</span>
        </div>
        <div ref={logRef} className="flex flex-col gap-1 h-24 overflow-y-auto pr-1">
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
