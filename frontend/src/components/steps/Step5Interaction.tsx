"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef } from "react"
import ReactMarkdown from "react-markdown"
import { Button } from "@/components/ui/button"
import { Send } from "lucide-react"
import { chatWithReport, getReport } from "@/lib/api/report"

interface Step5Props {
  reportId: string
  simulationId: string | null
  systemLogs: { time: string; msg: string }[]
  onAddLog: (msg: string) => void
  onUpdateStatus: (status: string) => void
}

interface ChatMessage {
  role: "user" | "assistant"
  content: string
  sources?: any[]
}

export default function Step5Interaction({ reportId, simulationId, systemLogs, onAddLog, onUpdateStatus }: Step5Props) {
  const logRef = useRef<HTMLDivElement>(null)
  const chatEndRef = useRef<HTMLDivElement>(null)
  const [messages, setMessages] = useState<ChatMessage[]>([])
  const [input, setInput] = useState("")
  const [sending, setSending] = useState(false)
  const [reportSummary, setReportSummary] = useState("")

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" })
  }, [messages])

  // Load report summary on mount
  useEffect(() => {
    const loadReport = async () => {
      if (!reportId) return
      try {
        const res: any = await getReport(reportId)
        if (res.success && res.data) {
          const content = res.data.content || res.data.report_content || ""
          setReportSummary(content.substring(0, 500) + (content.length > 500 ? "..." : ""))
          onAddLog("Report data loaded for interaction")
        }
      } catch (err: any) {
        onAddLog(`Failed to load report: ${err.message}`)
      }
    }
    loadReport()
  }, [reportId, onAddLog])

  const handleSend = async () => {
    if (!input.trim() || !simulationId || sending) return
    const userMessage = input.trim()
    setInput("")
    setMessages((prev) => [...prev, { role: "user", content: userMessage }])
    setSending(true)
    onAddLog(`User: ${userMessage.substring(0, 50)}...`)
    onUpdateStatus("processing")

    try {
      const chatHistory = messages.map((m) => ({ role: m.role, content: m.content }))
      const res: any = await chatWithReport({
        simulation_id: simulationId,
        message: userMessage,
        chat_history: chatHistory,
      })
      if (res.success && res.data) {
        const reply = res.data.response || res.data.message || "No response"
        setMessages((prev) => [
          ...prev,
          { role: "assistant", content: reply, sources: res.data.sources },
        ])
        onAddLog(`Assistant replied (${reply.length} chars)`)
      } else {
        setMessages((prev) => [...prev, { role: "assistant", content: "Error: " + (res.error || "Unknown error") }])
        onAddLog(`Chat error: ${res.error}`)
      }
    } catch (err: any) {
      setMessages((prev) => [...prev, { role: "assistant", content: "Error: " + err.message }])
      onAddLog(`Chat exception: ${err.message}`)
    } finally {
      setSending(false)
      onUpdateStatus("ready")
    }
  }

  return (
    <div className="h-full flex flex-col bg-white overflow-hidden">
      {/* Chat Header */}
      <div className="border-b border-gray-200 px-6 py-4 flex items-center justify-between flex-shrink-0">
        <div>
          <h3 className="font-semibold text-sm">Deep Interaction</h3>
          <p className="text-xs text-gray-400 font-mono mt-0.5">Chat with Report Agent</p>
        </div>
        <div className="text-xs text-gray-400 font-mono">
          Report: {reportId?.slice(0, 8)}...
        </div>
      </div>

      {/* Chat Messages */}
      <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {/* Report Summary Card */}
        {reportSummary && messages.length === 0 && (
          <div className="bg-gray-50 rounded-lg p-4 border border-gray-100">
            <div className="text-xs font-semibold text-gray-500 mb-2 uppercase tracking-wider">Report Summary</div>
            <p className="text-xs text-gray-600 leading-relaxed">{reportSummary}</p>
            <p className="text-xs text-primary mt-3 font-medium">Ask me anything about the simulation results...</p>
          </div>
        )}

        {messages.map((msg, i) => (
          <div key={i} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
            <div className={`max-w-[80%] rounded-lg px-4 py-3 ${
              msg.role === "user"
                ? "bg-primary text-white"
                : "bg-gray-50 border border-gray-200 text-gray-800"
            }`}>
              {msg.role === "assistant" ? (
                <div className="prose prose-sm max-w-none prose-p:text-gray-700 prose-p:my-1 prose-headings:text-gray-900 text-sm">
                  <ReactMarkdown>{msg.content}</ReactMarkdown>
                </div>
              ) : (
                <p className="text-sm whitespace-pre-wrap">{msg.content}</p>
              )}

              {msg.sources && msg.sources.length > 0 && (
                <div className="mt-3 pt-2 border-t border-gray-200">
                  <span className="text-[10px] text-gray-400 font-mono uppercase">Sources:</span>
                  <div className="mt-1 space-y-1">
                    {msg.sources.map((src: any, j: number) => (
                      <div key={j} className="text-[11px] text-gray-500 bg-white/50 rounded px-2 py-1">
                        {src.title || src.name || `Source ${j + 1}`}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          </div>
        ))}

        {sending && (
          <div className="flex justify-start">
            <div className="bg-gray-50 border border-gray-200 rounded-lg px-4 py-3">
              <div className="flex gap-1">
                <div className="w-2 h-2 bg-gray-300 rounded-full animate-bounce" style={{ animationDelay: "0ms" }} />
                <div className="w-2 h-2 bg-gray-300 rounded-full animate-bounce" style={{ animationDelay: "150ms" }} />
                <div className="w-2 h-2 bg-gray-300 rounded-full animate-bounce" style={{ animationDelay: "300ms" }} />
              </div>
            </div>
          </div>
        )}
        <div ref={chatEndRef} />
      </div>

      {/* Chat Input */}
      <div className="border-t border-gray-200 px-6 py-4 flex-shrink-0">
        <div className="flex gap-3">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && handleSend()}
            placeholder="Ask about the simulation results..."
            className="flex-1 border border-gray-200 rounded-lg px-4 py-2.5 text-sm outline-none focus:border-primary transition-colors"
            disabled={sending}
          />
          <Button onClick={handleSend} disabled={!input.trim() || sending} className="px-4">
            <Send size={16} />
          </Button>
        </div>
      </div>

      {/* System Logs */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span>INTERACTION AGENT</span>
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
