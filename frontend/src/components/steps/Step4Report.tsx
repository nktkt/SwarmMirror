"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback } from "react"
import { useRouter } from "next/navigation"
import ReactMarkdown from "react-markdown"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"
import { getReportStatus, getReport, getAgentLog } from "@/lib/api/report"

interface Step4Props {
  reportId: string
  simulationId: string | null
  systemLogs: { time: string; msg: string }[]
  onAddLog: (msg: string) => void
  onUpdateStatus: (status: string) => void
}

export default function Step4Report({ reportId, simulationId, systemLogs, onAddLog, onUpdateStatus }: Step4Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const [phase, setPhase] = useState(0) // 0: generating, 1: complete
  const [progress, setProgress] = useState(0)
  const [progressMsg, setProgressMsg] = useState("")
  const [reportContent, setReportContent] = useState("")
  const [agentLogs, setAgentLogs] = useState<string[]>([])
  const [showAgentLogs, setShowAgentLogs] = useState(false)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)
  const agentLogLineRef = useRef(0)

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  const stopPolling = useCallback(() => {
    if (pollRef.current) { clearInterval(pollRef.current); pollRef.current = null }
  }, [])

  useEffect(() => { return () => stopPolling() }, [stopPolling])

  const startPolling = useCallback(() => {
    const poll = async () => {
      try {
        const res: any = await getReportStatus(reportId)
        if (res.success && res.data) {
          const { status, progress: p, message, current_section, total_sections } = res.data
          setProgress(p || 0)
          if (message) setProgressMsg(message)
          if (current_section && total_sections) {
            onAddLog(`Section ${current_section}/${total_sections}: ${message || ""}`)
          }
          if (status === "completed") {
            setPhase(1)
            setProgress(100)
            stopPolling()
            onAddLog("Report generation complete")
            onUpdateStatus("completed")
            loadReport()
          } else if (status === "failed") {
            stopPolling()
            onAddLog(`Report generation failed: ${res.data.error}`)
            onUpdateStatus("error")
          }
        }
      } catch { /* ignore */ }

      // Agent log polling
      try {
        const logRes: any = await getAgentLog(reportId, agentLogLineRef.current)
        if (logRes.success && logRes.data?.lines) {
          setAgentLogs((prev) => [...prev, ...logRes.data.lines])
          agentLogLineRef.current += logRes.data.lines.length
        }
      } catch { /* ignore */ }
    }
    poll()
    pollRef.current = setInterval(poll, 3000)
  }, [reportId, onAddLog, onUpdateStatus, stopPolling])

  const loadReport = async () => {
    try {
      const res: any = await getReport(reportId)
      if (res.success && res.data) {
        setReportContent(res.data.content || res.data.report_content || "")
        onAddLog("Report content loaded")
      }
    } catch (err: any) {
      onAddLog(`Failed to load report: ${err.message}`)
    }
  }

  useEffect(() => {
    if (reportId) {
      onAddLog(`Monitoring report generation: ${reportId}`)
      startPolling()
    }
  }, [reportId, startPolling, onAddLog])

  return (
    <div className="h-full flex flex-col bg-[#FAFAFA] overflow-hidden">
      <div className="flex-1 overflow-y-auto p-6">
        {/* Progress Section */}
        {phase === 0 && (
          <div className="bg-white rounded-lg p-6 shadow-sm border border-gray-200 mb-6">
            <div className="flex justify-between items-center mb-4">
              <h3 className="font-semibold text-sm">Report Generation</h3>
              <Badge variant="processing">{progress}%</Badge>
            </div>
            <Progress value={progress} className="h-2 mb-3" />
            <p className="text-xs text-gray-500">{progressMsg || "Generating report..."}</p>

            <div className="mt-4">
              <button
                className="text-xs text-gray-400 hover:text-gray-600 font-mono underline"
                onClick={() => setShowAgentLogs(!showAgentLogs)}
              >
                {showAgentLogs ? "Hide" : "Show"} Agent Logs ({agentLogs.length} lines)
              </button>
              {showAgentLogs && (
                <div className="mt-2 bg-gray-900 text-green-400 font-mono text-[11px] p-3 rounded max-h-[200px] overflow-y-auto">
                  {agentLogs.map((line, i) => (
                    <div key={i} className="leading-relaxed">{line}</div>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}

        {/* Report Content */}
        {phase === 1 && reportContent && (
          <div className="bg-white rounded-lg shadow-sm border border-gray-200">
            <div className="flex justify-between items-center px-6 py-4 border-b border-gray-100">
              <div className="flex items-center gap-3">
                <Badge variant="success">Complete</Badge>
                <span className="text-sm font-semibold">Prediction Report</span>
              </div>
              {simulationId && (
                <Button
                  size="sm"
                  onClick={() => router.push(`/interaction/${reportId}`)}
                >
                  Deep Interaction &rarr;
                </Button>
              )}
            </div>
            <div className="p-6 prose prose-sm max-w-none prose-headings:text-gray-900 prose-p:text-gray-600 prose-strong:text-gray-800 prose-code:text-primary">
              <ReactMarkdown>{reportContent}</ReactMarkdown>
            </div>
          </div>
        )}

        {phase === 1 && !reportContent && (
          <div className="bg-white rounded-lg p-8 shadow-sm border border-gray-200 text-center">
            <div className="w-10 h-10 border-3 border-gray-200 border-t-primary rounded-full animate-spin mx-auto mb-4" />
            <p className="text-sm text-gray-500">Loading report content...</p>
          </div>
        )}
      </div>

      {/* System Logs */}
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
