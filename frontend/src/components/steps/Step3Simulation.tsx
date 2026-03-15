"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback, useMemo } from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  startSimulation,
  stopSimulation,
  getRunStatus,
  getRunStatusDetail,
} from "@/lib/api/simulation"
import { generateReport } from "@/lib/api/report"

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

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

interface ActionItem {
  _uniqueId: string
  id?: string
  agent_name?: string
  agent_id?: string
  action_type: string
  action_args?: Record<string, any>
  platform: "twitter" | "reddit"
  round_num?: number
  timestamp?: string
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

const ACTION_TYPE_LABELS: Record<string, string> = {
  CREATE_POST: "POST",
  REPOST: "REPOST",
  LIKE_POST: "LIKE",
  CREATE_COMMENT: "COMMENT",
  LIKE_COMMENT: "LIKE",
  DO_NOTHING: "IDLE",
  FOLLOW: "FOLLOW",
  SEARCH_POSTS: "SEARCH",
  QUOTE_POST: "QUOTE",
  UPVOTE_POST: "UPVOTE",
  DOWNVOTE_POST: "DOWNVOTE",
}

function getActionTypeLabel(type: string): string {
  return ACTION_TYPE_LABELS[type] || type || "UNKNOWN"
}

function getActionBadgeClasses(type: string): string {
  const map: Record<string, string> = {
    CREATE_POST: "bg-gray-100 text-gray-800 border-gray-200",
    QUOTE_POST: "bg-gray-100 text-gray-800 border-gray-200",
    REPOST: "bg-white text-gray-600 border-gray-200",
    LIKE_POST: "bg-white text-gray-600 border-gray-200",
    LIKE_COMMENT: "bg-white text-gray-600 border-gray-200",
    UPVOTE_POST: "bg-white text-gray-600 border-gray-200",
    DOWNVOTE_POST: "bg-white text-gray-600 border-gray-200",
    CREATE_COMMENT: "bg-gray-100 text-gray-600 border-gray-200",
    FOLLOW: "bg-gray-50 text-gray-400 border-dashed border-gray-300",
    SEARCH_POSTS: "bg-gray-50 text-gray-400 border-dashed border-gray-300",
    DO_NOTHING: "opacity-50 bg-gray-50 text-gray-400 border-gray-200",
  }
  return map[type] || "bg-gray-50 text-gray-500 border-gray-200"
}

function formatActionTime(ts: string): string {
  if (!ts) return ""
  try {
    return new Date(ts).toLocaleTimeString("en-US", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
  } catch {
    return ""
  }
}

function truncateContent(content: string, max = 100): string {
  if (!content) return ""
  return content.length > max ? content.substring(0, max) + "..." : content
}

/* ------------------------------------------------------------------ */
/*  Available-actions tooltips data                                    */
/* ------------------------------------------------------------------ */

const TWITTER_ACTIONS = ["POST", "LIKE", "REPOST", "QUOTE", "FOLLOW", "IDLE"]
const REDDIT_ACTIONS = ["POST", "COMMENT", "LIKE", "DISLIKE", "SEARCH", "TREND", "FOLLOW", "MUTE", "REFRESH", "IDLE"]

/* ------------------------------------------------------------------ */
/*  Sub-components                                                     */
/* ------------------------------------------------------------------ */

/** Platform status card shown in the top control bar */
function PlatformStatusCard({
  name,
  isActive,
  isCompleted,
  currentRound,
  totalRounds,
  elapsedTime,
  actionsCount,
  availableActions,
}: {
  name: string
  isActive: boolean
  isCompleted: boolean
  currentRound: number
  totalRounds: string | number
  elapsedTime: string
  actionsCount: number
  availableActions: string[]
}) {
  const [showTooltip, setShowTooltip] = useState(false)

  let borderClass = "border-gray-200 bg-gray-50 opacity-70"
  if (isActive) borderClass = "border-gray-800 bg-white opacity-100"
  if (isCompleted) borderClass = "border-green-600 bg-green-50 opacity-100"

  return (
    <div
      className={`relative flex flex-col gap-1 px-3 py-1.5 rounded border text-xs min-w-[160px] transition-all cursor-pointer ${borderClass}`}
      onMouseEnter={() => setShowTooltip(true)}
      onMouseLeave={() => setShowTooltip(false)}
    >
      {/* Header */}
      <div className="flex items-center gap-2 font-bold text-[11px] uppercase tracking-wide">
        {name}
        {isCompleted && (
          <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={3} className="text-green-600">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        )}
      </div>

      {/* Stats row */}
      <div className="flex gap-2.5 text-gray-500">
        <span>
          <span className="text-[8px] uppercase font-semibold tracking-wide mr-0.5">R</span>
          <span className="font-mono font-semibold text-gray-800">{currentRound}</span>
          <span className="text-[9px] text-gray-400">/{totalRounds}</span>
        </span>
        <span className="font-mono text-gray-500">{elapsedTime}</span>
        <span>
          <span className="text-[8px] uppercase font-semibold tracking-wide mr-0.5">A</span>
          <span className="font-mono font-semibold text-gray-800">{actionsCount}</span>
        </span>
      </div>

      {/* Actions tooltip */}
      {showTooltip && (
        <div className="absolute top-full left-1/2 -translate-x-1/2 mt-2 py-2.5 px-3.5 bg-black text-white rounded shadow-lg z-[100] min-w-[180px] pointer-events-none">
          <div className="absolute -top-1.5 left-1/2 -translate-x-1/2 w-0 h-0 border-l-[6px] border-r-[6px] border-b-[6px] border-transparent border-b-black" />
          <div className="text-[10px] font-semibold text-gray-400 uppercase tracking-wide mb-2">
            Available Actions
          </div>
          <div className="flex flex-wrap gap-1.5">
            {availableActions.map((action) => (
              <span
                key={action}
                className="text-[10px] font-semibold px-2 py-0.5 bg-white/15 rounded text-white tracking-wide"
              >
                {action}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/** Individual timeline card for an agent action */
function ActionCard({ action }: { action: ActionItem }) {
  const isTwitter = action.platform === "twitter"

  return (
    <div
      className={`flex justify-center mb-8 relative w-full ${
        isTwitter ? "pr-[50%]" : "pl-[50%]"
      }`}
    >
      {/* Center axis marker dot */}
      <div className="absolute left-1/2 top-6 w-2.5 h-2.5 bg-white border border-gray-400 rounded-full -translate-x-1/2 z-[2] flex items-center justify-center">
        <div className="w-1 h-1 bg-gray-800 rounded-full" />
      </div>

      {/* Card */}
      <div
        className={`w-[calc(100%-48px)] bg-white rounded border border-gray-200 p-4 hover:shadow-md transition-shadow ${
          isTwitter ? "ml-auto mr-8" : "mr-auto ml-8"
        }`}
      >
        {/* Card header: agent info + badge */}
        <div className="flex justify-between items-start mb-3 pb-3 border-b border-gray-100">
          <div className="flex items-center gap-2.5">
            <div className="w-6 h-6 bg-black text-white rounded-full flex items-center justify-center text-[11px] font-bold uppercase flex-shrink-0">
              {(action.agent_name || "A")[0]}
            </div>
            <span className="text-sm font-semibold">{action.agent_name}</span>
          </div>
          <div className="flex items-center gap-2">
            {/* Platform icon */}
            <div className="text-gray-400 flex items-center">
              {isTwitter ? (
                <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2}>
                  <circle cx="12" cy="12" r="10" />
                  <line x1="2" y1="12" x2="22" y2="12" />
                  <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                </svg>
              ) : (
                <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2}>
                  <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" />
                </svg>
              )}
            </div>
            {/* Action type badge */}
            <span
              className={`text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase tracking-wide border ${getActionBadgeClasses(
                action.action_type
              )}`}
            >
              {getActionTypeLabel(action.action_type)}
            </span>
          </div>
        </div>

        {/* Card body - content varies by action type */}
        <div className="text-sm text-gray-700 leading-relaxed">
          {/* CREATE_POST */}
          {action.action_type === "CREATE_POST" && action.action_args?.content && (
            <p className="text-gray-900 text-[14px]">{action.action_args.content}</p>
          )}

          {/* QUOTE_POST */}
          {action.action_type === "QUOTE_POST" && (
            <>
              {action.action_args?.quote_content && (
                <p className="mb-2">{action.action_args.quote_content}</p>
              )}
              {action.action_args?.original_content && (
                <div className="bg-gray-50 border border-gray-100 p-2.5 rounded mt-1 text-xs text-gray-500">
                  <div className="flex items-center gap-1.5 mb-1 text-gray-400">
                    <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2}>
                      <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
                      <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
                    </svg>
                    <span>@{action.action_args.original_author_name || "User"}</span>
                  </div>
                  <div>{truncateContent(action.action_args.original_content, 150)}</div>
                </div>
              )}
            </>
          )}

          {/* REPOST */}
          {action.action_type === "REPOST" && (
            <>
              <div className="flex items-center gap-1.5 text-[11px] text-gray-500 mb-1.5">
                <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                  <polyline points="17 1 21 5 17 9" />
                  <path d="M3 11V9a4 4 0 0 1 4-4h14" />
                  <polyline points="7 23 3 19 7 15" />
                  <path d="M21 13v2a4 4 0 0 1-4 4H3" />
                </svg>
                <span>Reposted from @{action.action_args?.original_author_name || "User"}</span>
              </div>
              {action.action_args?.original_content && (
                <div className="bg-gray-50 border border-gray-100 p-2.5 rounded text-xs text-gray-500">
                  {truncateContent(action.action_args.original_content, 200)}
                </div>
              )}
            </>
          )}

          {/* LIKE_POST */}
          {action.action_type === "LIKE_POST" && (
            <>
              <div className="flex items-center gap-1.5 text-[11px] text-gray-500 mb-1">
                <svg viewBox="0 0 24 24" width={14} height={14} fill="currentColor" className="text-gray-400">
                  <path d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 0 0 0-7.78z" />
                </svg>
                <span>
                  Liked @{action.action_args?.post_author_name || "User"}&apos;s post
                </span>
              </div>
              {action.action_args?.post_content && (
                <div className="text-xs text-gray-400 italic">
                  &ldquo;{truncateContent(action.action_args.post_content, 120)}&rdquo;
                </div>
              )}
            </>
          )}

          {/* CREATE_COMMENT */}
          {action.action_type === "CREATE_COMMENT" && (
            <>
              {action.action_args?.content && <p>{action.action_args.content}</p>}
              {action.action_args?.post_id && (
                <div className="flex items-center gap-1.5 text-[11px] text-gray-400 mt-1">
                  <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2}>
                    <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" />
                  </svg>
                  <span>Reply to post #{action.action_args.post_id}</span>
                </div>
              )}
            </>
          )}

          {/* SEARCH_POSTS */}
          {action.action_type === "SEARCH_POSTS" && (
            <div className="flex items-center gap-1.5 text-[11px] text-gray-500">
              <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                <circle cx="11" cy="11" r="8" />
                <line x1="21" y1="21" x2="16.65" y2="16.65" />
              </svg>
              <span>Search Query:</span>
              <span className="font-mono bg-gray-100 px-1 rounded">
                &ldquo;{action.action_args?.query || ""}&rdquo;
              </span>
            </div>
          )}

          {/* FOLLOW */}
          {action.action_type === "FOLLOW" && (
            <div className="flex items-center gap-1.5 text-[11px] text-gray-500">
              <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                <path d="M16 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
                <circle cx="8.5" cy="7" r="4" />
                <line x1="20" y1="8" x2="20" y2="14" />
                <line x1="23" y1="11" x2="17" y2="11" />
              </svg>
              <span>
                Followed @{action.action_args?.target_user || action.action_args?.user_id || "User"}
              </span>
            </div>
          )}

          {/* UPVOTE_POST / DOWNVOTE_POST */}
          {(action.action_type === "UPVOTE_POST" || action.action_type === "DOWNVOTE_POST") && (
            <>
              <div className="flex items-center gap-1.5 text-[11px] text-gray-500 mb-1">
                {action.action_type === "UPVOTE_POST" ? (
                  <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                    <polyline points="18 15 12 9 6 15" />
                  </svg>
                ) : (
                  <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                    <polyline points="6 9 12 15 18 9" />
                  </svg>
                )}
                <span>{action.action_type === "UPVOTE_POST" ? "Upvoted" : "Downvoted"} Post</span>
              </div>
              {action.action_args?.post_content && (
                <div className="text-xs text-gray-400 italic">
                  &ldquo;{truncateContent(action.action_args.post_content, 120)}&rdquo;
                </div>
              )}
            </>
          )}

          {/* DO_NOTHING */}
          {action.action_type === "DO_NOTHING" && (
            <div className="flex items-center gap-1.5 text-[11px] text-gray-400">
              <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="8" x2="12" y2="12" />
                <line x1="12" y1="16" x2="12.01" y2="16" />
              </svg>
              <span>Action Skipped</span>
            </div>
          )}

          {/* Fallback for unknown types */}
          {!["CREATE_POST", "QUOTE_POST", "REPOST", "LIKE_POST", "CREATE_COMMENT", "SEARCH_POSTS", "FOLLOW", "UPVOTE_POST", "DOWNVOTE_POST", "DO_NOTHING"].includes(
            action.action_type
          ) &&
            action.action_args?.content && <p>{action.action_args.content}</p>}
        </div>

        {/* Card footer */}
        <div className="mt-3 flex justify-end text-[10px] text-gray-300 font-mono">
          R{action.round_num} &bull; {formatActionTime(action.timestamp || "")}
        </div>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                     */
/* ------------------------------------------------------------------ */

export default function Step3Simulation({
  simulationId,
  maxRounds,
  minutesPerRound,
  systemLogs,
  onAddLog,
  onUpdateStatus,
}: Step3Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const scrollContainerRef = useRef<HTMLDivElement>(null)

  /* ---------- state ---------- */
  const [phase, setPhase] = useState(0) // 0=not started, 1=running, 2=completed
  const [isStarting, setIsStarting] = useState(false)
  const [isStopping, setIsStopping] = useState(false)
  const [startError, setStartError] = useState<string | null>(null)
  const [runStatus, setRunStatus] = useState<any>({})
  const [allActions, setAllActions] = useState<ActionItem[]>([])
  const [isGeneratingReport, setIsGeneratingReport] = useState(false)

  /* We use refs for mutable sets / timers to avoid stale closure issues */
  const actionIdsRef = useRef(new Set<string>())
  const statusTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const detailTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const prevTwitterRound = useRef(0)
  const prevRedditRound = useRef(0)

  /* ---------- derived ---------- */
  const twitterCount = useMemo(() => allActions.filter((a) => a.platform === "twitter").length, [allActions])
  const redditCount = useMemo(() => allActions.filter((a) => a.platform === "reddit").length, [allActions])

  const formatElapsedTime = useCallback(
    (round: number) => {
      if (!round || round <= 0) return "0h 0m"
      const total = round * minutesPerRound
      return `${Math.floor(total / 60)}h ${total % 60}m`
    },
    [minutesPerRound]
  )

  /* ---------- effects ---------- */
  // Auto-scroll system logs
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  // Cleanup polling on unmount
  const stopPolling = useCallback(() => {
    if (statusTimerRef.current) {
      clearInterval(statusTimerRef.current)
      statusTimerRef.current = null
    }
    if (detailTimerRef.current) {
      clearInterval(detailTimerRef.current)
      detailTimerRef.current = null
    }
  }, [])

  useEffect(() => {
    return () => stopPolling()
  }, [stopPolling])

  /* ---------- polling callbacks ---------- */
  const checkPlatformsCompleted = useCallback((data: any): boolean => {
    if (!data) return false
    const twitterCompleted = data.twitter_completed === true
    const redditCompleted = data.reddit_completed === true
    const twitterEnabled = (data.twitter_actions_count > 0) || data.twitter_running || twitterCompleted
    const redditEnabled = (data.reddit_actions_count > 0) || data.reddit_running || redditCompleted
    if (!twitterEnabled && !redditEnabled) return false
    if (twitterEnabled && !twitterCompleted) return false
    if (redditEnabled && !redditCompleted) return false
    return true
  }, [])

  const fetchRunStatus = useCallback(async () => {
    if (!simulationId) return
    try {
      const res: any = await getRunStatus(simulationId)
      if (res.success && res.data) {
        const data = res.data
        setRunStatus(data)

        // Log round changes
        if (data.twitter_current_round > prevTwitterRound.current) {
          onAddLog(
            `[Plaza] R${data.twitter_current_round}/${data.total_rounds} | T:${data.twitter_simulated_hours || 0}h | A:${data.twitter_actions_count}`
          )
          prevTwitterRound.current = data.twitter_current_round
        }
        if (data.reddit_current_round > prevRedditRound.current) {
          onAddLog(
            `[Community] R${data.reddit_current_round}/${data.total_rounds} | T:${data.reddit_simulated_hours || 0}h | A:${data.reddit_actions_count}`
          )
          prevRedditRound.current = data.reddit_current_round
        }

        // Check completion
        const isCompleted = data.runner_status === "completed" || data.runner_status === "stopped"
        const platformsDone = checkPlatformsCompleted(data)
        if (isCompleted || platformsDone) {
          if (platformsDone && !isCompleted) {
            onAddLog("All platforms completed")
          }
          onAddLog("Simulation completed")
          setPhase(2)
          stopPolling()
          onUpdateStatus("completed")
        }
      }
    } catch {
      /* ignore polling errors */
    }
  }, [simulationId, onAddLog, onUpdateStatus, stopPolling, checkPlatformsCompleted])

  const fetchRunStatusDetail = useCallback(async () => {
    if (!simulationId) return
    try {
      const res: any = await getRunStatusDetail(simulationId)
      if (res.success && res.data) {
        const serverActions = res.data.all_actions || []
        const newActions: ActionItem[] = []

        serverActions.forEach((action: any) => {
          const id =
            action.id ||
            `${action.timestamp}-${action.platform}-${action.agent_id}-${action.action_type}`
          if (!actionIdsRef.current.has(id)) {
            actionIdsRef.current.add(id)
            newActions.push({ ...action, _uniqueId: id })
          }
        })

        if (newActions.length > 0) {
          setAllActions((prev) => [...prev, ...newActions])
        }
      }
    } catch {
      /* ignore polling errors */
    }
  }, [simulationId])

  /* ---------- simulation control ---------- */
  const resetAllState = useCallback(() => {
    setPhase(0)
    setRunStatus({})
    setAllActions([])
    actionIdsRef.current = new Set()
    prevTwitterRound.current = 0
    prevRedditRound.current = 0
    setStartError(null)
    setIsStarting(false)
    setIsStopping(false)
    stopPolling()
  }, [stopPolling])

  const doStartSimulation = useCallback(async () => {
    if (!simulationId) {
      onAddLog("Error: missing simulationId")
      return
    }

    resetAllState()
    setIsStarting(true)
    setStartError(null)
    onAddLog("Starting dual-platform parallel simulation...")
    onUpdateStatus("processing")

    try {
      const params: any = {
        simulation_id: simulationId,
        platform: "parallel",
        force: true,
        enable_graph_memory_update: true,
      }
      if (maxRounds) {
        params.max_rounds = maxRounds
        onAddLog(`Max rounds set to: ${maxRounds}`)
      }
      onAddLog("Dynamic graph memory update enabled")

      const res: any = await startSimulation(params)
      if (res.success && res.data) {
        if (res.data.force_restarted) {
          onAddLog("Previous simulation logs cleared, restarting")
        }
        onAddLog("Simulation engine started successfully")
        onAddLog(`  PID: ${res.data.process_pid || "-"}`)

        setPhase(1)
        setRunStatus(res.data)

        // Start polling
        statusTimerRef.current = setInterval(fetchRunStatus, 2000)
        detailTimerRef.current = setInterval(fetchRunStatusDetail, 3000)
      } else {
        const errMsg = res.error || "Unknown error"
        setStartError(errMsg)
        onAddLog(`Start failed: ${errMsg}`)
        onUpdateStatus("error")
      }
    } catch (err: any) {
      setStartError(err.message)
      onAddLog(`Start error: ${err.message}`)
      onUpdateStatus("error")
    } finally {
      setIsStarting(false)
    }
  }, [
    simulationId,
    maxRounds,
    onAddLog,
    onUpdateStatus,
    resetAllState,
    fetchRunStatus,
    fetchRunStatusDetail,
  ])

  const handleStopSimulation = useCallback(async () => {
    if (!simulationId) return
    setIsStopping(true)
    onAddLog("Stopping simulation...")

    try {
      const res: any = await stopSimulation({ simulation_id: simulationId })
      if (res.success) {
        onAddLog("Simulation stopped")
        setPhase(2)
        stopPolling()
        onUpdateStatus("completed")
      } else {
        onAddLog(`Stop failed: ${res.error || "Unknown error"}`)
      }
    } catch (err: any) {
      onAddLog(`Stop error: ${err.message}`)
    } finally {
      setIsStopping(false)
    }
  }, [simulationId, onAddLog, stopPolling, onUpdateStatus])

  // Auto-start on mount if simulationId is present
  useEffect(() => {
    if (simulationId) {
      doStartSimulation()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [simulationId])

  /* ---------- report generation ---------- */
  const handleNextStep = useCallback(async () => {
    if (!simulationId || isGeneratingReport) return
    setIsGeneratingReport(true)
    onAddLog("Starting report generation...")

    try {
      const res: any = await generateReport({
        simulation_id: simulationId,
        force_regenerate: true,
      })
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
  }, [simulationId, isGeneratingReport, onAddLog, router])

  /* ------------------------------------------------------------------ */
  /*  Render                                                             */
  /* ------------------------------------------------------------------ */
  return (
    <div className="h-full flex flex-col bg-white overflow-hidden">
      {/* ===== Control Bar ===== */}
      <div className="bg-white px-6 py-3 flex justify-between items-center border-b border-gray-200 flex-shrink-0">
        <div className="flex gap-3">
          <PlatformStatusCard
            name="Info Plaza"
            isActive={!!runStatus.twitter_running}
            isCompleted={!!runStatus.twitter_completed}
            currentRound={runStatus.twitter_current_round || 0}
            totalRounds={runStatus.total_rounds || maxRounds || "-"}
            elapsedTime={formatElapsedTime(runStatus.twitter_current_round || 0)}
            actionsCount={runStatus.twitter_actions_count || 0}
            availableActions={TWITTER_ACTIONS}
          />
          <PlatformStatusCard
            name="Topic Community"
            isActive={!!runStatus.reddit_running}
            isCompleted={!!runStatus.reddit_completed}
            currentRound={runStatus.reddit_current_round || 0}
            totalRounds={runStatus.total_rounds || maxRounds || "-"}
            elapsedTime={formatElapsedTime(runStatus.reddit_current_round || 0)}
            actionsCount={runStatus.reddit_actions_count || 0}
            availableActions={REDDIT_ACTIONS}
          />
        </div>

        <div className="flex items-center gap-3">
          {/* Stop button (visible during running) */}
          {phase === 1 && (
            <Button
              variant="outline"
              size="sm"
              disabled={isStopping}
              onClick={handleStopSimulation}
              className="text-xs uppercase tracking-wide"
            >
              {isStopping ? "Stopping..." : "Stop"}
            </Button>
          )}

          {/* Error display */}
          {startError && (
            <span className="text-xs text-red-500 max-w-[200px] truncate" title={startError}>
              {startError}
            </span>
          )}

          {/* Generate Report button */}
          <Button
            disabled={phase !== 2 || isGeneratingReport}
            onClick={handleNextStep}
            className="uppercase tracking-wide text-xs"
          >
            {isGeneratingReport && (
              <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2" />
            )}
            {isGeneratingReport ? "Starting..." : "Generate Report"}
            {!isGeneratingReport && <span className="ml-1">&rarr;</span>}
          </Button>
        </div>
      </div>

      {/* ===== Timeline Feed ===== */}
      <div className="flex-1 overflow-y-auto relative bg-white" ref={scrollContainerRef}>
        {/* Stats header (sticky) */}
        {allActions.length > 0 && (
          <div className="sticky top-0 bg-white/90 backdrop-blur-sm py-3 px-6 border-b border-gray-200 z-10 flex justify-center">
            <div className="flex items-center gap-4 text-[11px] text-gray-500 bg-gray-100 px-3 py-1 rounded-full">
              <span className="font-semibold text-gray-800">
                TOTAL EVENTS: <span className="font-mono">{allActions.length}</span>
              </span>
              <span className="flex items-center gap-1">
                <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2} className="text-gray-800">
                  <circle cx="12" cy="12" r="10" />
                  <line x1="2" y1="12" x2="22" y2="12" />
                  <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                </svg>
                <span className="font-mono">{twitterCount}</span>
              </span>
              <span className="text-gray-300">/</span>
              <span className="flex items-center gap-1">
                <svg viewBox="0 0 24 24" width={12} height={12} fill="none" stroke="currentColor" strokeWidth={2} className="text-gray-800">
                  <path d="M21 11.5a8.38 8.38 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.38 8.38 0 0 1-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.38 8.38 0 0 1 3.8-.9h.5a8.48 8.48 0 0 1 8 8v.5z" />
                </svg>
                <span className="font-mono">{redditCount}</span>
              </span>
            </div>
          </div>
        )}

        {/* Timeline area */}
        <div className="py-6 relative max-w-[900px] mx-auto min-h-full">
          {/* Center vertical axis */}
          <div className="absolute left-1/2 top-0 bottom-0 w-px bg-gray-200 -translate-x-1/2" />

          {/* Action cards */}
          {allActions.map((action) => (
            <ActionCard key={action._uniqueId} action={action} />
          ))}

          {/* Waiting state */}
          {allActions.length === 0 && (
            <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col items-center gap-4 text-gray-300 text-xs uppercase tracking-widest">
              <div className="w-8 h-8 rounded-full border border-gray-200 animate-pulse" />
              <span>
                {phase === 0 && isStarting
                  ? "Starting simulation..."
                  : "Waiting for agent actions..."}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* ===== System Logs ===== */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span className="uppercase tracking-wider">SIMULATION MONITOR</span>
          <span>{simulationId || "NO_SIMULATION"}</span>
        </div>
        <div
          ref={logRef}
          className="flex flex-col gap-1 h-24 overflow-y-auto pr-1 scrollbar-thin"
        >
          {systemLogs.map((log, i) => (
            <div key={i} className="text-[11px] flex gap-3 leading-relaxed">
              <span className="text-gray-600 min-w-[75px] flex-shrink-0">{log.time}</span>
              <span className="text-gray-400 break-all">{log.msg}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
