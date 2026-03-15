"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback, useMemo } from "react"
import { useRouter } from "next/navigation"
import ReactMarkdown from "react-markdown"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { getAgentLog, getConsoleLog } from "@/lib/api/report"

// ─── Types ───────────────────────────────────────────────────────────
interface AgentLogEntry {
  action: string
  timestamp: string
  elapsed_seconds?: number
  section_index?: number
  section_title?: string
  details?: {
    simulation_id?: string
    simulation_requirement?: string
    message?: string
    outline?: { title: string; summary: string; sections: { title: string; description?: string }[] }
    tool_name?: string
    parameters?: any
    result?: string
    result_length?: number
    iteration?: number
    has_tool_calls?: boolean
    has_final_answer?: boolean
    response?: string
    content?: string
  }
}

interface Step4Props {
  reportId: string
  simulationId: string | null
  systemLogs: { time: string; msg: string }[]
  onAddLog: (msg: string) => void
  onUpdateStatus: (status: string) => void
}

// ─── Tool Configurations ─────────────────────────────────────────────
const toolConfig: Record<string, { name: string; color: string }> = {
  insight_forge: { name: "Deep Insight", color: "purple" },
  panorama_search: { name: "Panorama Search", color: "blue" },
  interview_agents: { name: "Agent Interview", color: "green" },
  quick_search: { name: "Quick Search", color: "orange" },
  get_graph_statistics: { name: "Graph Stats", color: "cyan" },
  get_entities_by_type: { name: "Entity Query", color: "pink" },
}

const getToolDisplayName = (toolName?: string) => toolConfig[toolName || ""]?.name || toolName || "Unknown"
const getToolColor = (toolName?: string): string => toolConfig[toolName || ""]?.color || "gray"

const toolColorClasses: Record<string, string> = {
  purple: "bg-purple-50 text-purple-700 border-purple-200",
  blue: "bg-blue-50 text-blue-700 border-blue-200",
  green: "bg-green-50 text-green-700 border-green-200",
  orange: "bg-orange-50 text-orange-700 border-orange-200",
  cyan: "bg-cyan-50 text-cyan-700 border-cyan-200",
  pink: "bg-pink-50 text-pink-700 border-pink-200",
  gray: "bg-gray-50 text-gray-700 border-gray-200",
}

// ─── Component ───────────────────────────────────────────────────────
export default function Step4Report({
  reportId,
  simulationId,
  systemLogs,
  onAddLog,
  onUpdateStatus,
}: Step4Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const rightPanelRef = useRef<HTMLDivElement>(null)
  const consoleLogRef = useRef<HTMLDivElement>(null)

  // ─── State ────────────────────────────────────────────────────────
  const [agentLogs, setAgentLogs] = useState<AgentLogEntry[]>([])
  const [consoleLogs, setConsoleLogs] = useState<string[]>([])
  const agentLogLineRef = useRef(0)
  const consoleLogLineRef = useRef(0)
  const [reportOutline, setReportOutline] = useState<any>(null)
  const [currentSectionIndex, setCurrentSectionIndex] = useState<number | null>(null)
  const [generatedSections, setGeneratedSections] = useState<Record<number, string>>({})
  const [expandedLogs, setExpandedLogs] = useState<Set<string>>(new Set())
  const [collapsedSections, setCollapsedSections] = useState<Set<number>>(new Set())
  const [showRawResult, setShowRawResult] = useState<Record<string, boolean>>({})
  const [isComplete, setIsComplete] = useState(false)
  const [startTime, setStartTime] = useState<Date | null>(null)

  // Polling refs
  const agentLogTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const consoleLogTimerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // ─── Auto-scroll system logs ──────────────────────────────────────
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  // ─── Computed values ──────────────────────────────────────────────
  const totalSections = useMemo(() => reportOutline?.sections?.length || 0, [reportOutline])

  const completedSections = useMemo(() => Object.keys(generatedSections).length, [generatedSections])

  const totalToolCalls = useMemo(
    () => agentLogs.filter((l) => l.action === "tool_call").length,
    [agentLogs]
  )

  const formatElapsedTime = useMemo(() => {
    if (!startTime) return "0s"
    const lastLog = agentLogs[agentLogs.length - 1]
    const elapsed = lastLog?.elapsed_seconds || 0
    if (elapsed < 60) return `${Math.round(elapsed)}s`
    const mins = Math.floor(elapsed / 60)
    const secs = Math.round(elapsed % 60)
    return `${mins}m ${secs}s`
  }, [startTime, agentLogs])

  const statusClass = useMemo(() => {
    if (isComplete) return "completed"
    if (agentLogs.length > 0) return "processing"
    return "pending"
  }, [isComplete, agentLogs.length])

  const statusText = useMemo(() => {
    if (isComplete) return "Completed"
    if (agentLogs.length > 0) return "Generating..."
    return "Waiting"
  }, [isComplete, agentLogs.length])

  const isPlanningDone = useMemo(
    () => !!reportOutline?.sections?.length || agentLogs.some((l) => l.action === "planning_complete"),
    [reportOutline, agentLogs]
  )

  const isPlanningStarted = useMemo(
    () => agentLogs.some((l) => l.action === "planning_start" || l.action === "report_start"),
    [agentLogs]
  )

  const activeSectionIndex = useMemo(() => {
    if (isComplete) return null
    if (currentSectionIndex) return currentSectionIndex
    if (totalSections > 0 && completedSections < totalSections) return completedSections + 1
    return null
  }, [isComplete, currentSectionIndex, totalSections, completedSections])

  const isFinalizing = useMemo(
    () => !isComplete && isPlanningDone && totalSections > 0 && completedSections >= totalSections,
    [isComplete, isPlanningDone, totalSections, completedSections]
  )

  // Active step for header
  const activeStep = useMemo(() => {
    const steps = workflowSteps
    const active = steps.find((s) => s.status === "active")
    if (active) return active
    const doneSteps = steps.filter((s) => s.status === "done")
    if (doneSteps.length > 0) return doneSteps[doneSteps.length - 1]
    return steps[0] || { noLabel: "--", title: "Waiting...", status: "todo", meta: "" }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportOutline, isComplete, activeSectionIndex, isPlanningDone, isPlanningStarted, isFinalizing, generatedSections])

  // Workflow steps
  const workflowSteps = useMemo(() => {
    const steps: { key: string; noLabel: string; title: string; status: string; meta: string }[] = []

    const planningStatus = isPlanningDone ? "done" : isPlanningStarted ? "active" : "todo"
    steps.push({
      key: "planning",
      noLabel: "PL",
      title: "Planning / Outline",
      status: planningStatus,
      meta: planningStatus === "active" ? "IN PROGRESS" : "",
    })

    const sections = reportOutline?.sections || []
    sections.forEach((section: any, i: number) => {
      const idx = i + 1
      const status =
        isComplete || !!generatedSections[idx]
          ? "done"
          : activeSectionIndex === idx
          ? "active"
          : "todo"
      steps.push({
        key: `section-${idx}`,
        noLabel: String(idx).padStart(2, "0"),
        title: section.title,
        status,
        meta: status === "active" ? "IN PROGRESS" : "",
      })
    })

    const completeStatus = isComplete ? "done" : isFinalizing ? "active" : "todo"
    steps.push({
      key: "complete",
      noLabel: "OK",
      title: "Complete",
      status: completeStatus,
      meta: completeStatus === "active" ? "FINALIZING" : "",
    })

    return steps
  }, [reportOutline, isComplete, generatedSections, activeSectionIndex, isPlanningDone, isPlanningStarted, isFinalizing])

  // ─── Toggle helpers ───────────────────────────────────────────────
  const toggleLogExpand = useCallback((timestamp: string) => {
    setExpandedLogs((prev) => {
      const next = new Set(prev)
      if (next.has(timestamp)) next.delete(timestamp)
      else next.add(timestamp)
      return next
    })
  }, [])

  const toggleSectionCollapse = useCallback((idx: number) => {
    setCollapsedSections((prev) => {
      const next = new Set(prev)
      if (next.has(idx)) next.delete(idx)
      else next.add(idx)
      return next
    })
  }, [])

  const toggleRawResult = useCallback((timestamp: string) => {
    setShowRawResult((prev) => ({ ...prev, [timestamp]: !prev[timestamp] }))
  }, [])

  // ─── Format helpers ───────────────────────────────────────────────
  const formatTime = (timestamp?: string) => {
    if (!timestamp) return ""
    try {
      return new Date(timestamp).toLocaleTimeString("en-US", {
        hour12: false,
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      })
    } catch {
      return ""
    }
  }

  const formatParams = (params: any) => {
    if (!params) return ""
    try { return JSON.stringify(params, null, 2) } catch { return String(params) }
  }

  const formatResultSize = (length?: number) => {
    if (!length) return ""
    if (length < 1000) return `${length} chars`
    return `${(length / 1000).toFixed(1)}k chars`
  }

  const truncateText = (text?: string, maxLen = 300) => {
    if (!text) return ""
    if (text.length <= maxLen) return text
    return text.substring(0, maxLen) + "..."
  }

  const getActionLabel = (action: string) => {
    const labels: Record<string, string> = {
      report_start: "Report Started",
      planning_start: "Planning",
      planning_complete: "Plan Complete",
      section_start: "Section Start",
      section_content: "Content Ready",
      section_complete: "Section Done",
      tool_call: "Tool Call",
      tool_result: "Tool Result",
      llm_response: "LLM Response",
      report_complete: "Complete",
    }
    return labels[action] || action
  }

  const getLogLevelClass = (log: string) => {
    if (log.includes("ERROR")) return "text-red-400"
    if (log.includes("WARNING")) return "text-amber-400"
    return ""
  }

  const isSectionCompleted = useCallback((sectionIndex: number) => !!generatedSections[sectionIndex], [generatedSections])

  // ─── Timeline item classification ────────────────────────────────
  const getTimelineItemClass = (log: AgentLogEntry, idx: number, total: number) => {
    const isLatest = idx === total - 1 && !isComplete
    const isMilestone = log.action === "section_complete" || log.action === "report_complete"
    if (isLatest) return "border-gray-900 bg-gray-50"
    if (isMilestone) return "border-gray-200 bg-gray-50"
    return "border-gray-100 bg-white"
  }

  const getConnectorDotClass = (log: AgentLogEntry, idx: number, total: number) => {
    const isLatest = idx === total - 1 && !isComplete
    if (isLatest) return "bg-gray-900 shadow-[0_0_0_3px_rgba(59,130,246,0.12)]"
    if (log.action === "section_complete" || log.action === "report_complete") return "bg-emerald-500"
    return "bg-gray-300"
  }

  // ─── Polling ──────────────────────────────────────────────────────
  const stopPolling = useCallback(() => {
    if (agentLogTimerRef.current) { clearInterval(agentLogTimerRef.current); agentLogTimerRef.current = null }
    if (consoleLogTimerRef.current) { clearInterval(consoleLogTimerRef.current); consoleLogTimerRef.current = null }
  }, [])

  useEffect(() => () => stopPolling(), [stopPolling])

  const fetchAgentLog = useCallback(async () => {
    if (!reportId) return
    try {
      const res: any = await getAgentLog(reportId, agentLogLineRef.current)
      if (res.success && res.data) {
        const newLogs: AgentLogEntry[] = res.data.logs || []
        if (newLogs.length > 0) {
          setAgentLogs((prev) => {
            const updated = [...prev, ...newLogs]
            // Process each new log
            newLogs.forEach((log) => {
              if (log.action === "planning_complete" && log.details?.outline) {
                setReportOutline(log.details.outline)
              }
              if (log.action === "section_start") {
                setCurrentSectionIndex(log.section_index || null)
              }
              if (log.action === "section_complete" && log.details?.content) {
                setGeneratedSections((prev) => ({ ...prev, [log.section_index!]: log.details!.content! }))
                setCurrentSectionIndex(null)
              }
              if (log.action === "report_complete") {
                setIsComplete(true)
                setCurrentSectionIndex(null)
                onUpdateStatus("completed")
                stopPolling()
              }
              if (log.action === "report_start") {
                setStartTime(new Date(log.timestamp))
              }
            })
            return updated
          })
          agentLogLineRef.current = res.data.from_line + newLogs.length

          // Auto-scroll right panel
          setTimeout(() => {
            if (rightPanelRef.current) {
              rightPanelRef.current.scrollTop = rightPanelRef.current.scrollHeight
            }
          }, 50)
        }
      }
    } catch { /* ignore */ }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId, stopPolling])

  const fetchConsoleLog = useCallback(async () => {
    if (!reportId) return
    try {
      const res: any = await getConsoleLog(reportId, consoleLogLineRef.current)
      if (res.success && res.data) {
        const newLogs: string[] = res.data.logs || []
        if (newLogs.length > 0) {
          setConsoleLogs((prev) => [...prev, ...newLogs])
          consoleLogLineRef.current = res.data.from_line + newLogs.length
          setTimeout(() => {
            if (consoleLogRef.current) {
              consoleLogRef.current.scrollTop = consoleLogRef.current.scrollHeight
            }
          }, 50)
        }
      }
    } catch { /* ignore */ }
  }, [reportId])

  const startPolling = useCallback(() => {
    if (agentLogTimerRef.current || consoleLogTimerRef.current) return
    fetchAgentLog()
    fetchConsoleLog()
    agentLogTimerRef.current = setInterval(fetchAgentLog, 2000)
    consoleLogTimerRef.current = setInterval(fetchConsoleLog, 1500)
  }, [fetchAgentLog, fetchConsoleLog])

  // Start polling on mount
  useEffect(() => {
    if (reportId) {
      onAddLog(`Report Agent initialized: ${reportId}`)
      startPolling()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId])

  // Reset on reportId change
  useEffect(() => {
    if (reportId) {
      setAgentLogs([])
      setConsoleLogs([])
      agentLogLineRef.current = 0
      consoleLogLineRef.current = 0
      setReportOutline(null)
      setCurrentSectionIndex(null)
      setGeneratedSections({})
      setExpandedLogs(new Set())
      setCollapsedSections(new Set())
      setIsComplete(false)
      setStartTime(null)
      setShowRawResult({})
      startPolling()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId])

  // ─── Navigation ───────────────────────────────────────────────────
  const goToInteraction = () => {
    if (reportId) router.push(`/interaction/${reportId}`)
  }

  // Download report as .md
  const downloadReport = () => {
    if (!reportOutline || completedSections === 0) return
    let md = `# ${reportOutline.title}\n\n${reportOutline.summary}\n\n`
    reportOutline.sections.forEach((section: any, i: number) => {
      md += `## ${i + 1}. ${section.title}\n\n`
      if (generatedSections[i + 1]) md += generatedSections[i + 1] + "\n\n"
    })
    const blob = new Blob([md], { type: "text/markdown" })
    const url = URL.createObjectURL(blob)
    const a = document.createElement("a")
    a.href = url
    a.download = `report-${reportId}.md`
    a.click()
    URL.revokeObjectURL(url)
  }

  // ─── Render: Tool Call Badge ──────────────────────────────────────
  const renderToolBadge = (log: AgentLogEntry) => {
    const toolName = log.details?.tool_name
    const color = getToolColor(toolName)
    const classes = toolColorClasses[color] || toolColorClasses.gray
    return (
      <div className={`inline-flex items-center gap-1.5 text-xs font-medium px-2 py-1 rounded border ${classes}`}>
        {getToolDisplayName(toolName)}
      </div>
    )
  }

  // ─── Render: Timeline Entry ───────────────────────────────────────
  const renderTimelineEntry = (log: AgentLogEntry, idx: number) => {
    const total = agentLogs.length
    const isExpanded = expandedLogs.has(log.timestamp)

    return (
      <div
        key={`${log.timestamp}-${idx}`}
        className={`grid grid-cols-[24px_1fr] gap-3 p-2.5 mb-2.5 border rounded-lg transition-colors ${getTimelineItemClass(log, idx, total)}`}
      >
        {/* Connector */}
        <div className="flex flex-col items-center w-6 flex-shrink-0">
          <div className={`w-3 h-3 rounded-full border-2 border-white z-[1] ${getConnectorDotClass(log, idx, total)}`} />
          {idx < total - 1 && <div className="w-0.5 flex-1 bg-gray-100 -mt-0.5" />}
        </div>

        {/* Content */}
        <div className="min-w-0">
          {/* Header */}
          <div className="flex justify-between items-center mb-2">
            <span className="text-xs font-semibold text-gray-700 uppercase tracking-wider">{getActionLabel(log.action)}</span>
            <span className="font-mono text-[10px] text-gray-400">{formatTime(log.timestamp)}</span>
          </div>

          {/* Body */}
          <div
            className={`${["tool_call", "tool_result", "llm_response"].includes(log.action) && !isExpanded ? "cursor-pointer" : ""}`}
            onClick={() => {
              if (["tool_call", "tool_result", "llm_response"].includes(log.action)) toggleLogExpand(log.timestamp)
            }}
          >
            {/* Report Start */}
            {log.action === "report_start" && (
              <div className="space-y-1.5">
                <div className="flex justify-between text-xs">
                  <span className="text-gray-400">Simulation</span>
                  <span className="font-mono text-gray-600">{log.details?.simulation_id}</span>
                </div>
                {log.details?.simulation_requirement && (
                  <div className="flex justify-between text-xs">
                    <span className="text-gray-400">Requirement</span>
                    <span className="text-gray-600">{log.details.simulation_requirement}</span>
                  </div>
                )}
              </div>
            )}

            {/* Planning */}
            {log.action === "planning_start" && (
              <div className="text-xs text-indigo-600 bg-indigo-50 px-2 py-1.5 rounded">{log.details?.message}</div>
            )}
            {log.action === "planning_complete" && (
              <div className="space-y-1.5">
                <div className="text-xs text-emerald-600 bg-emerald-50 px-2 py-1.5 rounded">{log.details?.message}</div>
                {log.details?.outline && (
                  <span className="inline-block text-[11px] font-mono bg-gray-100 text-gray-600 px-2 py-0.5 rounded">
                    {log.details.outline.sections?.length || 0} sections planned
                  </span>
                )}
              </div>
            )}

            {/* Section Start */}
            {log.action === "section_start" && (
              <div className="flex items-center gap-2">
                <span className="font-mono text-xs text-gray-400">#{log.section_index}</span>
                <span className="text-xs text-gray-700 font-medium">{log.section_title}</span>
              </div>
            )}

            {/* Section Content */}
            {log.action === "section_content" && (
              <div className="flex items-center gap-2 text-emerald-600">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
                <span className="text-xs font-medium">{log.section_title}</span>
              </div>
            )}

            {/* Section Complete */}
            {log.action === "section_complete" && (
              <div className="flex items-center gap-2 text-emerald-600">
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="20 6 9 17 4 12"/></svg>
                <span className="text-xs font-medium">{log.section_title}</span>
              </div>
            )}

            {/* Tool Call */}
            {log.action === "tool_call" && (
              <div>
                {renderToolBadge(log)}
                {isExpanded && log.details?.parameters && (
                  <pre className="mt-2 text-[11px] font-mono bg-gray-50 p-2 rounded border border-gray-200 overflow-x-auto max-h-[200px] overflow-y-auto">
                    {formatParams(log.details.parameters)}
                  </pre>
                )}
              </div>
            )}

            {/* Tool Result */}
            {log.action === "tool_result" && (
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs text-gray-500 font-medium">{getToolDisplayName(log.details?.tool_name)}</span>
                  <span className="font-mono text-[10px] text-gray-400">{formatResultSize(log.details?.result_length)}</span>
                </div>
                {showRawResult[log.timestamp] ? (
                  <pre className="text-[11px] font-mono bg-gray-50 p-2 rounded border border-gray-200 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap">
                    {log.details?.result}
                  </pre>
                ) : (
                  <pre className="text-[11px] font-mono bg-gray-50 p-2 rounded border border-gray-200 overflow-x-auto max-h-[150px] overflow-y-auto whitespace-pre-wrap text-gray-500">
                    {truncateText(log.details?.result, 300)}
                  </pre>
                )}
              </div>
            )}

            {/* LLM Response */}
            {log.action === "llm_response" && (
              <div>
                <div className="flex flex-wrap gap-1.5 mb-1.5">
                  <span className="text-[10px] font-mono bg-gray-100 text-gray-600 px-1.5 py-0.5 rounded">
                    Iteration {log.details?.iteration}
                  </span>
                  <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded ${log.details?.has_tool_calls ? "bg-blue-50 text-blue-600" : "bg-gray-100 text-gray-400"}`}>
                    Tools: {log.details?.has_tool_calls ? "Yes" : "No"}
                  </span>
                  <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded ${log.details?.has_final_answer ? "bg-emerald-50 text-emerald-600 font-semibold" : "bg-gray-100 text-gray-400"}`}>
                    Final: {log.details?.has_final_answer ? "Yes" : "No"}
                  </span>
                </div>
                {log.details?.has_final_answer && (
                  <div className="flex items-center gap-1.5 text-emerald-600 text-xs mt-1">
                    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2"><polyline points="20 6 9 17 4 12"/></svg>
                    <span>Section &ldquo;{log.section_title}&rdquo; content generated</span>
                  </div>
                )}
                {isExpanded && log.details?.response && (
                  <pre className="mt-2 text-[11px] font-mono bg-gray-50 p-2 rounded border border-gray-200 overflow-x-auto max-h-[300px] overflow-y-auto whitespace-pre-wrap">
                    {log.details.response}
                  </pre>
                )}
              </div>
            )}

            {/* Report Complete */}
            {log.action === "report_complete" && (
              <div className="flex items-center gap-2 text-emerald-600">
                <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                </svg>
                <span className="text-sm font-semibold">Report Generation Complete</span>
              </div>
            )}
          </div>

          {/* Footer */}
          {(log.elapsed_seconds || log.action === "tool_call" || log.action === "tool_result" || log.action === "llm_response") && (
            <div className="flex justify-between items-center mt-2 pt-1.5">
              {log.elapsed_seconds ? (
                <span className="font-mono text-[10px] text-gray-400">+{log.elapsed_seconds.toFixed(1)}s</span>
              ) : (
                <span />
              )}
              <div className="flex gap-2">
                {log.action === "tool_call" && log.details?.parameters && (
                  <button
                    className="text-[10px] text-gray-400 hover:text-gray-600 font-mono"
                    onClick={(e) => { e.stopPropagation(); toggleLogExpand(log.timestamp) }}
                  >
                    {isExpanded ? "Hide Params" : "Show Params"}
                  </button>
                )}
                {log.action === "tool_result" && (
                  <button
                    className="text-[10px] text-gray-400 hover:text-gray-600 font-mono"
                    onClick={(e) => { e.stopPropagation(); toggleRawResult(log.timestamp) }}
                  >
                    {showRawResult[log.timestamp] ? "Truncated" : "Full Output"}
                  </button>
                )}
                {log.action === "llm_response" && log.details?.response && (
                  <button
                    className="text-[10px] text-gray-400 hover:text-gray-600 font-mono"
                    onClick={(e) => { e.stopPropagation(); toggleLogExpand(log.timestamp) }}
                  >
                    {isExpanded ? "Hide Response" : "Show Response"}
                  </button>
                )}
              </div>
            </div>
          )}
        </div>
      </div>
    )
  }

  // ═══════════════════════════════════════════════════════════════════
  // RENDER
  // ═══════════════════════════════════════════════════════════════════
  return (
    <div className="h-full flex flex-col bg-[#F8F9FA] overflow-hidden">

      {/* ═══ Main Split Layout ═══ */}
      <div className="flex-1 flex overflow-hidden">

        {/* ═══ LEFT PANEL: Report Content ═══ */}
        <div className="w-[45%] min-w-[450px] bg-white border-r border-gray-200 overflow-y-auto px-[50px] py-[30px]">
          {reportOutline ? (
            <div className="max-w-[800px] mx-auto w-full">
              {/* Report Header */}
              <div className="mb-8">
                <div className="flex items-center gap-3 mb-6">
                  <span className="bg-black text-white text-[11px] font-bold px-2 py-1 uppercase tracking-wider">Prediction Report</span>
                  <span className="text-[11px] text-gray-400 font-medium tracking-wider">ID: {reportId || "REF-2024-X92"}</span>
                </div>
                <h1 className="font-serif text-4xl font-bold text-gray-900 leading-tight mb-4 tracking-tight">
                  {reportOutline.title}
                </h1>
                <p className="font-serif text-base text-gray-500 italic leading-relaxed mb-8">
                  {reportOutline.summary}
                </p>
                <div className="h-px bg-gray-200 w-full" />
              </div>

              {/* Sections */}
              <div className="space-y-8">
                {reportOutline.sections.map((section: any, idx: number) => {
                  const sectionIdx = idx + 1
                  const completed = isSectionCompleted(sectionIdx)
                  const isActive = currentSectionIndex === sectionIdx
                  const isPending = !completed && !isActive
                  return (
                    <div key={idx}>
                      {/* Section Header */}
                      <div
                        className={`flex items-baseline gap-3 p-2 -mx-3 rounded-lg transition-colors ${completed ? "cursor-pointer hover:bg-gray-50" : ""}`}
                        onClick={() => { if (completed) toggleSectionCollapse(idx) }}
                      >
                        <span className="font-mono text-base text-gray-400 font-medium">
                          {String(sectionIdx).padStart(2, "0")}
                        </span>
                        <h3 className={`font-serif text-2xl font-semibold m-0 transition-colors ${isPending ? "text-gray-300" : "text-gray-900"}`}>
                          {section.title}
                        </h3>
                        {completed && (
                          <svg
                            className={`ml-auto text-gray-400 transition-transform flex-shrink-0 ${collapsedSections.has(idx) ? "-rotate-90" : ""}`}
                            viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2"
                          >
                            <polyline points="6 9 12 15 18 9" />
                          </svg>
                        )}
                      </div>

                      {/* Section Body */}
                      {!collapsedSections.has(idx) && (
                        <div className="pl-7 mt-3">
                          {completed && generatedSections[sectionIdx] ? (
                            <div className="prose prose-sm max-w-none prose-headings:font-serif prose-headings:text-gray-900 prose-p:text-gray-600 prose-p:leading-relaxed prose-strong:text-gray-900 prose-blockquote:text-gray-500 prose-blockquote:border-l-gray-200 prose-blockquote:italic prose-blockquote:font-serif prose-li:text-gray-600 prose-code:text-gray-700 prose-code:bg-gray-50 prose-code:rounded prose-code:px-1 prose-pre:bg-gray-50 prose-pre:border prose-pre:border-gray-200 text-sm leading-relaxed">
                              <ReactMarkdown>{generatedSections[sectionIdx]}</ReactMarkdown>
                            </div>
                          ) : isActive ? (
                            <div className="flex items-center gap-2.5 text-gray-500 text-sm mt-1">
                              <div className="w-[18px] h-[18px] animate-spin flex items-center justify-center">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" className="w-full h-full">
                                  <circle cx="12" cy="12" r="10" strokeWidth="4" stroke="#E5E7EB" />
                                  <path d="M12 2a10 10 0 0 1 10 10" strokeWidth="4" stroke="#4B5563" strokeLinecap="round" />
                                </svg>
                              </div>
                              <span className="font-serif text-[15px] text-gray-600">
                                Generating {section.title}...
                              </span>
                            </div>
                          ) : null}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>

              {/* Download Button */}
              {isComplete && completedSections > 0 && (
                <div className="mt-8 pt-6 border-t border-gray-200 flex gap-3">
                  <Button variant="outline" size="sm" onClick={downloadReport}>
                    Download Report (.md)
                  </Button>
                  {simulationId && (
                    <Button size="sm" onClick={goToInteraction}>
                      Deep Interaction &rarr;
                    </Button>
                  )}
                </div>
              )}
            </div>
          ) : (
            /* Waiting State */
            <div className="flex-1 flex flex-col items-center justify-center gap-5 p-10 text-gray-400 h-full">
              <div className="relative w-12 h-12">
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping" style={{ animationDuration: "2s" }} />
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping" style={{ animationDuration: "2s", animationDelay: "0.4s" }} />
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping" style={{ animationDuration: "2s", animationDelay: "0.8s" }} />
              </div>
              <span className="text-sm">Waiting for Report Agent...</span>
            </div>
          )}
        </div>

        {/* ═══ RIGHT PANEL: Workflow Timeline ═══ */}
        <div ref={rightPanelRef} className="flex-1 bg-white overflow-y-auto flex flex-col">

          {/* Active Step Header */}
          {!isComplete && activeStep && (
            <div className={`flex items-center gap-2.5 px-5 py-3.5 bg-white border-b border-gray-200 text-[13px] font-semibold text-gray-700 uppercase tracking-wider sticky top-0 z-10 ${activeStep.status === "active" ? "bg-gray-50 border-gray-900" : ""}`}>
              {activeStep.status === "active" && (
                <span className="w-2 h-2 rounded-full bg-gray-900 shadow-[0_0_0_3px_rgba(31,41,55,0.15)] mr-2.5 flex-shrink-0 animate-pulse" />
              )}
              <span className="font-mono text-xs text-gray-400 mr-2.5 flex-shrink-0">{activeStep.noLabel}</span>
              <span className="text-[13px] font-semibold text-gray-700 normal-case tracking-normal truncate">{activeStep.title}</span>
              {activeStep.meta && (
                <span className="ml-auto font-mono text-[10px] font-bold text-gray-500 flex-shrink-0">{activeStep.meta}</span>
              )}
            </div>
          )}

          {/* Workflow Overview */}
          {(agentLogs.length > 0 || reportOutline) && (
            <div className="px-5 pt-4">
              {/* Metrics */}
              <div className="flex flex-wrap items-center gap-2.5 mb-3">
                <div className="inline-flex items-baseline gap-1.5">
                  <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">Sections</span>
                  <span className="font-mono text-xs text-gray-700">{completedSections}/{totalSections}</span>
                </div>
                <div className="inline-flex items-baseline gap-1.5">
                  <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">Elapsed</span>
                  <span className="font-mono text-xs text-gray-700">{formatElapsedTime}</span>
                </div>
                <div className="inline-flex items-baseline gap-1.5">
                  <span className="text-[11px] font-semibold text-gray-400 uppercase tracking-wider">Tools</span>
                  <span className="font-mono text-xs text-gray-700">{totalToolCalls}</span>
                </div>
                <div className="ml-auto">
                  <span className={`text-[11px] font-bold uppercase tracking-wider px-2.5 py-1 rounded-full border ${
                    statusClass === "completed"
                      ? "bg-emerald-50 border-emerald-200 text-emerald-700"
                      : statusClass === "processing"
                      ? "bg-gray-50 border-gray-900 text-gray-900"
                      : "bg-transparent border-gray-300 border-dashed text-gray-500"
                  }`}>
                    {statusText}
                  </span>
                </div>
              </div>

              {/* Workflow Steps */}
              {workflowSteps.length > 0 && (
                <div className="space-y-2.5 pb-2.5">
                  {workflowSteps.map((step, sidx) => (
                    <div
                      key={step.key}
                      className={`grid grid-cols-[24px_1fr] gap-3 p-2.5 border rounded-lg ${
                        step.status === "active"
                          ? "bg-gray-50 border-gray-900"
                          : step.status === "done"
                          ? "bg-gray-50 border-gray-200"
                          : "bg-transparent border-gray-200 border-dashed"
                      }`}
                    >
                      <div className="flex flex-col items-center w-6 flex-shrink-0">
                        <div className={`w-2.5 h-2.5 rounded-full border-2 border-white z-[1] ${
                          step.status === "active"
                            ? "bg-gray-900 shadow-[0_0_0_3px_rgba(59,130,246,0.12)]"
                            : step.status === "done"
                            ? "bg-emerald-500"
                            : "bg-gray-300"
                        }`} />
                        {sidx < workflowSteps.length - 1 && <div className="w-0.5 flex-1 bg-gray-100 -mt-0.5" />}
                      </div>
                      <div className="flex items-baseline gap-2.5 min-w-0">
                        <span className={`font-mono text-[11px] font-bold flex-shrink-0 ${step.status === "todo" ? "text-gray-400" : "text-gray-400"}`}>
                          {step.noLabel}
                        </span>
                        <span className={`font-serif text-[13px] font-semibold truncate ${step.status === "todo" ? "text-gray-400" : "text-gray-900"}`}>
                          {step.title}
                        </span>
                        {step.meta && (
                          <span className="ml-auto font-mono text-[10px] font-bold text-gray-900 uppercase tracking-wider flex-shrink-0">
                            {step.meta}
                          </span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}

              {/* Next Step Button */}
              {isComplete && (
                <button
                  className="w-full flex items-center justify-center gap-2 py-2.5 mt-2 bg-black text-white rounded-lg text-sm font-semibold hover:opacity-80 transition-opacity"
                  onClick={goToInteraction}
                >
                  <span>Deep Interaction</span>
                  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2">
                    <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                  </svg>
                </button>
              )}

              <div className="h-px bg-gray-100 mt-3.5" />
            </div>
          )}

          {/* Timeline */}
          <div className="px-5 py-3.5 flex-1">
            {agentLogs.map((log, idx) => renderTimelineEntry(log, idx))}

            {/* Empty State */}
            {agentLogs.length === 0 && !isComplete && (
              <div className="flex flex-col items-center justify-center gap-3 py-12 text-gray-400">
                <div className="w-3 h-3 rounded-full bg-gray-300 animate-pulse" />
                <span className="text-sm">Waiting for agent activity...</span>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* ═══ Console Logs ═══ */}
      <div className="bg-black text-gray-300 font-mono flex-shrink-0">
        <div className="flex justify-between items-center px-4 pt-3 pb-2 border-b border-gray-800 text-[10px] text-gray-600">
          <span>CONSOLE OUTPUT</span>
          <span>{reportId || "NO_REPORT"}</span>
        </div>
        <div ref={consoleLogRef} className="px-4 py-2 h-20 overflow-y-auto">
          {consoleLogs.map((log, idx) => (
            <div key={idx} className="text-[11px] leading-relaxed">
              <span className={`${getLogLevelClass(log)}`}>{log}</span>
            </div>
          ))}
          {consoleLogs.length === 0 && (
            <div className="text-[11px] text-gray-600">Waiting for console output...</div>
          )}
        </div>
      </div>

      {/* ═══ System Logs ═══ */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span>REPORT GENERATOR</span>
          <span>{reportId || "NO_REPORT"}</span>
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
