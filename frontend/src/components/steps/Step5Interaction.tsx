"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useRef, useCallback, useMemo } from "react"
import ReactMarkdown from "react-markdown"
import { Button } from "@/components/ui/button"
import { Send, ChevronDown } from "lucide-react"
import { chatWithReport, getReport, getAgentLog } from "@/lib/api/report"
import { interviewAgents, getSimulationProfilesRealtime } from "@/lib/api/simulation"

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

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
  timestamp?: string
}

interface AgentProfile {
  username?: string
  name?: string
  profession?: string
  bio?: string
  [key: string]: any
}

interface ReportOutline {
  title: string
  summary: string
  sections: { title: string; [key: string]: any }[]
}

interface SurveyResult {
  agent_id: number
  agent_name: string
  profession?: string
  question: string
  answer: string
}

/* ------------------------------------------------------------------ */
/*  Tool descriptions for the Report Agent card                        */
/* ------------------------------------------------------------------ */

const REPORT_AGENT_TOOLS = [
  {
    color: "purple",
    bgColor: "bg-purple-500/10",
    textColor: "text-purple-500",
    name: "InsightForge",
    desc: "Deep causal analysis aligning real-world seed data with simulation state, leveraging Global/Local Memory for cross-temporal attribution.",
    icon: (
      <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2}>
        <path d="M9 18h6M10 22h4M12 2a7 7 0 0 0-4 12.5V17a1 1 0 0 0 1 1h6a1 1 0 0 0 1-1v-2.5A7 7 0 0 0 12 2z" />
      </svg>
    ),
  },
  {
    color: "blue",
    bgColor: "bg-blue-500/10",
    textColor: "text-blue-500",
    name: "PanoramaSearch",
    desc: "Graph-based BFS algorithm reconstructing event propagation paths and capturing complete information flow topology.",
    icon: (
      <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2}>
        <circle cx="12" cy="12" r="10" />
        <path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
      </svg>
    ),
  },
  {
    color: "orange",
    bgColor: "bg-orange-500/10",
    textColor: "text-orange-500",
    name: "QuickSearch",
    desc: "GraphRAG-based instant query interface with optimized indexing for rapid extraction of node attributes and discrete facts.",
    icon: (
      <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2}>
        <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
      </svg>
    ),
  },
  {
    color: "green",
    bgColor: "bg-green-500/10",
    textColor: "text-green-500",
    name: "InterviewSubAgent",
    desc: "Autonomous interview system capable of parallel multi-round conversations with simulated agents for collecting unstructured opinion data.",
    icon: (
      <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="currentColor" strokeWidth={2}>
        <path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2" />
        <circle cx="9" cy="7" r="4" />
        <path d="M23 21v-2a4 4 0 0 0-3-3.87M16 3.13a4 4 0 0 1 0 7.75" />
      </svg>
    ),
  },
]

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function formatTime(timestamp: string | undefined): string {
  if (!timestamp) return ""
  try {
    return new Date(timestamp).toLocaleTimeString("en-US", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return ""
  }
}

/* ------------------------------------------------------------------ */
/*  Sub-components                                                     */
/* ------------------------------------------------------------------ */

/** Report Agent tools expandable card */
function ReportAgentToolsCard() {
  const [expanded, setExpanded] = useState(true)

  return (
    <div className="border-b border-gray-200 bg-gradient-to-br from-gray-50 to-gray-100/50">
      <div className="flex items-center gap-3 px-5 py-3.5">
        <div className="w-11 h-11 min-w-[44px] bg-gradient-to-br from-gray-800 to-gray-600 text-white rounded-full flex items-center justify-center text-lg font-semibold shadow-md flex-shrink-0">
          R
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[15px] font-semibold text-gray-800 mb-0.5">Report Agent - Chat</div>
          <div className="text-xs text-gray-500">
            Chat version of the Report Agent with 4 professional tools and full memory access.
          </div>
        </div>
        <button
          className="w-7 h-7 bg-white border border-gray-200 rounded-md flex items-center justify-center text-gray-500 hover:bg-gray-50 transition-all flex-shrink-0"
          onClick={() => setExpanded(!expanded)}
        >
          <ChevronDown
            size={16}
            className={`transition-transform duration-300 ${expanded ? "rotate-180" : ""}`}
          />
        </button>
      </div>
      {expanded && (
        <div className="px-5 pb-4">
          <div className="grid grid-cols-2 gap-2.5">
            {REPORT_AGENT_TOOLS.map((tool) => (
              <div
                key={tool.name}
                className="flex gap-2.5 p-3 bg-white rounded-lg border border-gray-200 hover:shadow-sm transition-shadow"
              >
                <div
                  className={`w-8 h-8 min-w-[32px] rounded-lg flex items-center justify-center flex-shrink-0 ${tool.bgColor} ${tool.textColor}`}
                >
                  {tool.icon}
                </div>
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-semibold text-gray-800 mb-1">{tool.name}</div>
                  <div className="text-[11px] text-gray-500 leading-snug line-clamp-2">
                    {tool.desc}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/** Agent profile card displayed when chatting with a specific agent */
function AgentProfileCard({ agent }: { agent: AgentProfile }) {
  const [expanded, setExpanded] = useState(true)

  return (
    <div className="border-b border-gray-200 bg-gradient-to-br from-gray-50 to-gray-100/50">
      <div className="flex items-center gap-3 px-5 py-3.5">
        <div className="w-11 h-11 min-w-[44px] bg-gradient-to-br from-gray-800 to-gray-600 text-white rounded-full flex items-center justify-center text-lg font-semibold shadow-md flex-shrink-0">
          {(agent.username || "A")[0]}
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[15px] font-semibold text-gray-800 mb-0.5">
            {agent.username}
          </div>
          <div className="flex items-center gap-2 text-xs text-gray-500">
            {agent.name && <span className="text-gray-400">@{agent.name}</span>}
            {agent.profession && (
              <span className="px-2 py-0.5 bg-gray-200 rounded text-[11px] font-medium">
                {agent.profession}
              </span>
            )}
          </div>
        </div>
        <button
          className="w-7 h-7 bg-white border border-gray-200 rounded-md flex items-center justify-center text-gray-500 hover:bg-gray-50 transition-all flex-shrink-0"
          onClick={() => setExpanded(!expanded)}
        >
          <ChevronDown
            size={16}
            className={`transition-transform duration-300 ${expanded ? "rotate-180" : ""}`}
          />
        </button>
      </div>
      {expanded && agent.bio && (
        <div className="px-5 pb-4">
          <div className="bg-white p-3 rounded-lg border border-gray-200">
            <div className="text-[11px] font-semibold text-gray-400 uppercase tracking-wide mb-1.5">
              Bio
            </div>
            <p className="text-[13px] text-gray-600 leading-relaxed m-0">{agent.bio}</p>
          </div>
        </div>
      )}
    </div>
  )
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                     */
/* ------------------------------------------------------------------ */

export default function Step5Interaction({
  reportId,
  simulationId,
  systemLogs,
  onAddLog,
  onUpdateStatus,
}: Step5Props) {
  const logRef = useRef<HTMLDivElement>(null)
  const chatMessagesRef = useRef<HTMLDivElement>(null)
  const chatInputRef = useRef<HTMLTextAreaElement>(null)

  /* ---------- Tab / Target state ---------- */
  const [activeTab, setActiveTab] = useState<"chat" | "survey">("chat")
  const [chatTarget, setChatTarget] = useState<"report_agent" | "agent">("report_agent")
  const [showAgentDropdown, setShowAgentDropdown] = useState(false)
  const [selectedAgent, setSelectedAgent] = useState<AgentProfile | null>(null)
  const [selectedAgentIndex, setSelectedAgentIndex] = useState<number | null>(null)

  /* ---------- Chat state ---------- */
  const [chatInput, setChatInput] = useState("")
  const [chatHistory, setChatHistory] = useState<ChatMessage[]>([])
  const [chatHistoryCache, setChatHistoryCache] = useState<Record<string, ChatMessage[]>>({})
  const [isSending, setIsSending] = useState(false)

  /* ---------- Survey state ---------- */
  const [selectedAgents, setSelectedAgents] = useState<Set<number>>(new Set())
  const [surveyQuestion, setSurveyQuestion] = useState("")
  const [surveyResults, setSurveyResults] = useState<SurveyResult[]>([])
  const [isSurveying, setIsSurveying] = useState(false)

  /* ---------- Report & Profiles state ---------- */
  const [reportOutline, setReportOutline] = useState<ReportOutline | null>(null)
  const [generatedSections, setGeneratedSections] = useState<Record<number, string>>({})
  const [collapsedSections, setCollapsedSections] = useState<Set<number>>(new Set())
  const [currentSectionIndex, setCurrentSectionIndex] = useState<number | null>(null)
  const [profiles, setProfiles] = useState<AgentProfile[]>([])

  /* ---------- Effects ---------- */
  // Auto-scroll system logs
  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  // Auto-scroll chat messages
  useEffect(() => {
    if (chatMessagesRef.current) {
      chatMessagesRef.current.scrollTop = chatMessagesRef.current.scrollHeight
    }
  }, [chatHistory, isSending])

  // Load report data on mount / reportId change
  useEffect(() => {
    if (!reportId) return
    const loadReportData = async () => {
      try {
        onAddLog(`Loading report data: ${reportId}`)
        const reportRes: any = await getReport(reportId)
        if (reportRes.success && reportRes.data) {
          await loadAgentLogs()
        }
      } catch (err: any) {
        onAddLog(`Failed to load report: ${err.message}`)
      }
    }

    const loadAgentLogs = async () => {
      try {
        const res: any = await getAgentLog(reportId, 0)
        if (res.success && res.data) {
          const logs = res.data.logs || []
          let outline: ReportOutline | null = null
          const sections: Record<number, string> = {}
          let activeSectionIdx: number | null = null

          logs.forEach((log: any) => {
            if (log.action === "planning_complete" && log.details?.outline) {
              outline = log.details.outline
            }
            if (
              log.action === "section_complete" &&
              log.section_index < 100 &&
              log.details?.content
            ) {
              sections[log.section_index] = log.details.content
            }
            if (log.action === "section_start" && log.section_index < 100) {
              activeSectionIdx = log.section_index
            }
          })

          if (outline) setReportOutline(outline)
          if (Object.keys(sections).length > 0) setGeneratedSections(sections)
          if (activeSectionIdx !== null && !sections[activeSectionIdx]) {
            setCurrentSectionIndex(activeSectionIdx)
          }
          onAddLog("Report data loaded")
        }
      } catch (err: any) {
        onAddLog(`Failed to load report logs: ${err.message}`)
      }
    }

    loadReportData()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reportId])

  // Load agent profiles on mount / simulationId change
  useEffect(() => {
    if (!simulationId) return
    const loadProfiles = async () => {
      try {
        const res: any = await getSimulationProfilesRealtime(simulationId, "reddit")
        if (res.success && res.data) {
          setProfiles(res.data.profiles || [])
          onAddLog(`Loaded ${(res.data.profiles || []).length} agent profiles`)
        }
      } catch (err: any) {
        onAddLog(`Failed to load profiles: ${err.message}`)
      }
    }
    loadProfiles()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [simulationId])

  // Close dropdown on outside click
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      const dropdown = document.querySelector("[data-agent-dropdown]")
      if (dropdown && !dropdown.contains(e.target as Node)) {
        setShowAgentDropdown(false)
      }
    }
    document.addEventListener("click", handleClickOutside)
    return () => document.removeEventListener("click", handleClickOutside)
  }, [])

  /* ---------- Chat history cache management ---------- */
  const saveChatHistory = useCallback(() => {
    if (chatHistory.length === 0) return
    setChatHistoryCache((prev) => {
      const key =
        chatTarget === "report_agent" ? "report_agent" : `agent_${selectedAgentIndex}`
      return { ...prev, [key]: [...chatHistory] }
    })
  }, [chatHistory, chatTarget, selectedAgentIndex])

  /* ---------- Tab / Target selection ---------- */
  const selectReportAgentChat = useCallback(() => {
    saveChatHistory()
    setActiveTab("chat")
    setChatTarget("report_agent")
    setSelectedAgent(null)
    setSelectedAgentIndex(null)
    setShowAgentDropdown(false)
    setChatHistory(chatHistoryCache["report_agent"] || [])
  }, [saveChatHistory, chatHistoryCache])

  const selectSurveyTab = useCallback(() => {
    saveChatHistory()
    setActiveTab("survey")
    setSelectedAgent(null)
    setSelectedAgentIndex(null)
    setShowAgentDropdown(false)
  }, [saveChatHistory])

  const toggleAgentDropdown = useCallback(() => {
    setShowAgentDropdown((prev) => {
      if (!prev) {
        setActiveTab("chat")
        setChatTarget("agent")
      }
      return !prev
    })
  }, [])

  const selectAgent = useCallback(
    (agent: AgentProfile, idx: number) => {
      saveChatHistory()
      setSelectedAgent(agent)
      setSelectedAgentIndex(idx)
      setChatTarget("agent")
      setShowAgentDropdown(false)
      setChatHistory(chatHistoryCache[`agent_${idx}`] || [])
      onAddLog(`Selected chat target: ${agent.username}`)
    },
    [saveChatHistory, chatHistoryCache, onAddLog]
  )

  /* ---------- Section collapse ---------- */
  const isSectionCompleted = useCallback(
    (sectionIndex: number) => !!generatedSections[sectionIndex],
    [generatedSections]
  )

  const toggleSectionCollapse = useCallback(
    (idx: number) => {
      if (!generatedSections[idx + 1]) return
      setCollapsedSections((prev) => {
        const next = new Set(prev)
        if (next.has(idx)) next.delete(idx)
        else next.add(idx)
        return next
      })
    },
    [generatedSections]
  )

  /* ---------- Send message ---------- */
  const sendMessage = useCallback(async () => {
    if (!chatInput.trim() || isSending) return
    const message = chatInput.trim()
    setChatInput("")

    const userMsg: ChatMessage = {
      role: "user",
      content: message,
      timestamp: new Date().toISOString(),
    }
    setChatHistory((prev) => [...prev, userMsg])
    setIsSending(true)

    try {
      if (chatTarget === "report_agent") {
        // Send to Report Agent
        onAddLog(`To Report Agent: ${message.substring(0, 50)}...`)
        const historyForApi = chatHistory
          .filter((msg) => msg.role !== "user" || msg.content !== message)
          .slice(-10)
          .map((msg) => ({ role: msg.role, content: msg.content }))

        const res: any = await chatWithReport({
          simulation_id: simulationId!,
          message,
          chat_history: historyForApi,
        })

        if (res.success && res.data) {
          const reply = res.data.response || res.data.answer || "No response"
          setChatHistory((prev) => [
            ...prev,
            {
              role: "assistant",
              content: reply,
              sources: res.data.sources,
              timestamp: new Date().toISOString(),
            },
          ])
          onAddLog("Report Agent replied")
        } else {
          throw new Error(res.error || "Request failed")
        }
      } else {
        // Send to selected agent
        if (!selectedAgent || selectedAgentIndex === null) {
          throw new Error("Please select an agent first")
        }
        onAddLog(`To ${selectedAgent.username}: ${message.substring(0, 50)}...`)

        // Build prompt with history context
        let prompt = message
        if (chatHistory.length > 1) {
          const historyContext = chatHistory
            .filter((msg) => msg.content !== message)
            .slice(-6)
            .map((msg) => `${msg.role === "user" ? "Interviewer" : "You"}: ${msg.content}`)
            .join("\n")
          prompt = `Previous conversation:\n${historyContext}\n\nNew question: ${message}`
        }

        const res: any = await interviewAgents({
          simulation_id: simulationId!,
          interviews: [{ agent_id: String(selectedAgentIndex), prompt }],
        })

        if (res.success && res.data) {
          const resultData = res.data.result || res.data
          const resultsDict = resultData.results || resultData
          let responseContent: string | null = null
          const agentId = selectedAgentIndex

          if (typeof resultsDict === "object" && !Array.isArray(resultsDict)) {
            const redditKey = `reddit_${agentId}`
            const twitterKey = `twitter_${agentId}`
            const agentResult =
              resultsDict[redditKey] || resultsDict[twitterKey] || Object.values(resultsDict)[0]
            if (agentResult) {
              responseContent =
                (agentResult as any).response || (agentResult as any).answer || null
            }
          } else if (Array.isArray(resultsDict) && resultsDict.length > 0) {
            responseContent = resultsDict[0].response || resultsDict[0].answer || null
          }

          if (responseContent) {
            setChatHistory((prev) => [
              ...prev,
              {
                role: "assistant",
                content: responseContent!,
                timestamp: new Date().toISOString(),
              },
            ])
            onAddLog(`${selectedAgent.username} replied`)
          } else {
            throw new Error("No response data")
          }
        } else {
          throw new Error(res.error || "Request failed")
        }
      }
    } catch (err: any) {
      onAddLog(`Send failed: ${err.message}`)
      setChatHistory((prev) => [
        ...prev,
        {
          role: "assistant",
          content: `Error: ${err.message}`,
          timestamp: new Date().toISOString(),
        },
      ])
    } finally {
      setIsSending(false)
      // Auto-save chat history
      saveChatHistory()
    }
  }, [
    chatInput,
    isSending,
    chatTarget,
    chatHistory,
    simulationId,
    selectedAgent,
    selectedAgentIndex,
    onAddLog,
    saveChatHistory,
  ])

  /* ---------- Survey methods ---------- */
  const toggleAgentSelection = useCallback((idx: number) => {
    setSelectedAgents((prev) => {
      const next = new Set(prev)
      if (next.has(idx)) next.delete(idx)
      else next.add(idx)
      return next
    })
  }, [])

  const selectAllAgents = useCallback(() => {
    setSelectedAgents(new Set(profiles.map((_, i) => i)))
  }, [profiles])

  const clearAgentSelection = useCallback(() => {
    setSelectedAgents(new Set())
  }, [])

  const submitSurvey = useCallback(async () => {
    if (selectedAgents.size === 0 || !surveyQuestion.trim()) return
    setIsSurveying(true)
    onAddLog(`Sending survey to ${selectedAgents.size} agents...`)

    try {
      const interviews = Array.from(selectedAgents).map((idx) => ({
        agent_id: String(idx),
        prompt: surveyQuestion.trim(),
      }))

      const res: any = await interviewAgents({
        simulation_id: simulationId!,
        interviews,
      })

      if (res.success && res.data) {
        const resultData = res.data.result || res.data
        const resultsDict = resultData.results || resultData
        const results: SurveyResult[] = []

        for (const interview of interviews) {
          const agentIdx = Number(interview.agent_id)
          const agent = profiles[agentIdx]
          let responseContent = "No response"

          if (typeof resultsDict === "object" && !Array.isArray(resultsDict)) {
            const redditKey = `reddit_${agentIdx}`
            const twitterKey = `twitter_${agentIdx}`
            const agentResult = resultsDict[redditKey] || resultsDict[twitterKey]
            if (agentResult) {
              responseContent = agentResult.response || agentResult.answer || "No response"
            }
          } else if (Array.isArray(resultsDict)) {
            const matched = resultsDict.find((r: any) => r.agent_id === agentIdx)
            if (matched) {
              responseContent = matched.response || matched.answer || "No response"
            }
          }

          results.push({
            agent_id: agentIdx,
            agent_name: agent?.username || `Agent ${agentIdx}`,
            profession: agent?.profession,
            question: surveyQuestion.trim(),
            answer: responseContent,
          })
        }

        setSurveyResults(results)
        onAddLog(`Received ${results.length} survey responses`)
      } else {
        throw new Error(res.error || "Request failed")
      }
    } catch (err: any) {
      onAddLog(`Survey failed: ${err.message}`)
    } finally {
      setIsSurveying(false)
    }
  }, [selectedAgents, surveyQuestion, simulationId, profiles, onAddLog])

  /* ---------- Derived ---------- */
  const profileCount = useMemo(() => profiles.length, [profiles])

  /* ------------------------------------------------------------------ */
  /*  Render                                                             */
  /* ------------------------------------------------------------------ */
  return (
    <div className="h-full flex flex-col bg-gray-50 overflow-hidden">
      {/* ===== Main Split Layout ===== */}
      <div className="flex-1 flex overflow-hidden">
        {/* ── LEFT PANEL: Report Sections ── */}
        <div className="w-[45%] min-w-[450px] bg-white border-r border-gray-200 overflow-y-auto flex flex-col py-8 px-12">
          {reportOutline ? (
            <div className="max-w-[800px] mx-auto w-full">
              {/* Report header */}
              <div className="mb-8">
                <div className="flex items-center gap-3 mb-6">
                  <span className="bg-black text-white text-[11px] font-bold px-2 py-1 uppercase tracking-wide">
                    Prediction Report
                  </span>
                  <span className="text-[11px] text-gray-400 font-medium">
                    ID: {reportId?.slice(0, 12) || "REF-X92"}
                  </span>
                </div>
                <h1
                  className="text-4xl font-bold text-gray-900 leading-tight mb-4"
                  style={{ fontFamily: "'Times New Roman', Times, serif", letterSpacing: "-0.02em" }}
                >
                  {reportOutline.title}
                </h1>
                <p
                  className="text-base text-gray-500 italic leading-relaxed mb-8"
                  style={{ fontFamily: "'Times New Roman', Times, serif" }}
                >
                  {reportOutline.summary}
                </p>
                <div className="h-px bg-gray-200 w-full" />
              </div>

              {/* Sections */}
              <div className="flex flex-col gap-8">
                {reportOutline.sections.map((section, idx) => {
                  const sectionIdx = idx + 1
                  const isActive = currentSectionIndex === sectionIdx
                  const isCompleted = isSectionCompleted(sectionIdx)
                  const isPending = !isCompleted && !isActive
                  const isCollapsed = collapsedSections.has(idx)

                  return (
                    <div key={idx} className="flex flex-col gap-3">
                      {/* Section header */}
                      <div
                        className={`flex items-baseline gap-3 py-2 px-3 -mx-3 rounded-lg transition-colors ${
                          isCompleted ? "cursor-pointer hover:bg-gray-50" : ""
                        }`}
                        onClick={() => toggleSectionCollapse(idx)}
                      >
                        <span
                          className={`font-mono text-base font-medium transition-colors ${
                            isPending ? "text-gray-200" : "text-gray-400"
                          }`}
                        >
                          {String(sectionIdx).padStart(2, "0")}
                        </span>
                        <h3
                          className={`text-2xl font-semibold m-0 transition-colors ${
                            isPending ? "text-gray-300" : "text-gray-900"
                          }`}
                          style={{ fontFamily: "'Times New Roman', Times, serif" }}
                        >
                          {section.title}
                        </h3>
                        {isCompleted && (
                          <ChevronDown
                            size={20}
                            className={`ml-auto text-gray-400 flex-shrink-0 transition-transform duration-300 ${
                              isCollapsed ? "-rotate-90" : ""
                            }`}
                          />
                        )}
                      </div>

                      {/* Section body */}
                      {!isCollapsed && (
                        <div className="pl-7">
                          {isCompleted && generatedSections[sectionIdx] && (
                            <div className="prose prose-sm max-w-none prose-p:text-gray-600 prose-p:my-3 prose-headings:text-gray-900 prose-strong:text-gray-900 prose-li:text-gray-600 text-sm leading-relaxed">
                              <ReactMarkdown>{generatedSections[sectionIdx]}</ReactMarkdown>
                            </div>
                          )}
                          {isActive && !isCompleted && (
                            <div className="flex items-center gap-2.5 text-gray-500 text-sm mt-1">
                              <div className="w-[18px] h-[18px] animate-spin flex items-center justify-center">
                                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" width={18} height={18}>
                                  <circle cx="12" cy="12" r="10" strokeWidth={4} stroke="#E5E7EB" />
                                  <path d="M12 2a10 10 0 0 1 10 10" strokeWidth={4} stroke="#4B5563" strokeLinecap="round" />
                                </svg>
                              </div>
                              <span style={{ fontFamily: "'Times New Roman', Times, serif" }}>
                                Generating {section.title}...
                              </span>
                            </div>
                          )}
                        </div>
                      )}
                    </div>
                  )
                })}
              </div>
            </div>
          ) : (
            /* Waiting state */
            <div className="flex-1 flex flex-col items-center justify-center gap-5 text-gray-400">
              <div className="relative w-12 h-12">
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping opacity-75" />
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping opacity-50" style={{ animationDelay: "0.4s" }} />
                <div className="absolute inset-0 border-2 border-gray-200 rounded-full animate-ping opacity-25" style={{ animationDelay: "0.8s" }} />
              </div>
              <span className="text-sm">Waiting for Report Agent...</span>
            </div>
          )}
        </div>

        {/* ── RIGHT PANEL: Interaction Interface ── */}
        <div className="flex-1 flex flex-col bg-white overflow-hidden">
          {/* Action bar with tabs */}
          <div className="flex items-center justify-between px-5 py-3.5 border-b border-gray-200 bg-gradient-to-b from-white to-gray-50/80 gap-4">
            {/* Left: title */}
            <div className="flex items-center gap-3 min-w-[160px]">
              <svg viewBox="0 0 24 24" width={28} height={28} fill="none" stroke="currentColor" strokeWidth={1.5} className="text-gray-800 flex-shrink-0">
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
              </svg>
              <div className="flex flex-col gap-0.5">
                <span className="text-[13px] font-semibold text-gray-800">Interactive Tools</span>
                <span className="text-[11px] text-gray-400 font-mono">{profileCount} agents available</span>
              </div>
            </div>

            {/* Right: tab pills */}
            <div className="flex items-center gap-1.5 flex-1 justify-end">
              {/* Report Agent tab */}
              <button
                className={`flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-full transition-all whitespace-nowrap ${
                  activeTab === "chat" && chatTarget === "report_agent"
                    ? "bg-gray-800 text-white shadow-md"
                    : "bg-gray-100 text-gray-500 hover:bg-gray-200"
                }`}
                onClick={selectReportAgentChat}
              >
                <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                  <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" />
                </svg>
                <span>Report Agent</span>
              </button>

              {/* Agent selector dropdown */}
              {profiles.length > 0 && (
                <div className="relative" data-agent-dropdown>
                  <button
                    className={`flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-full transition-all w-[200px] justify-between ${
                      activeTab === "chat" && chatTarget === "agent"
                        ? "bg-gray-800 text-white shadow-md"
                        : "bg-gray-100 text-gray-500 hover:bg-gray-200"
                    }`}
                    onClick={toggleAgentDropdown}
                  >
                    <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2} className="flex-shrink-0">
                      <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                      <circle cx="12" cy="7" r="4" />
                    </svg>
                    <span className="flex-1 text-left truncate">
                      {selectedAgent ? selectedAgent.username : "Chat with Agent"}
                    </span>
                    <ChevronDown
                      size={12}
                      className={`flex-shrink-0 opacity-60 transition-transform ${
                        showAgentDropdown ? "rotate-180" : ""
                      }`}
                    />
                  </button>

                  {/* Dropdown menu */}
                  {showAgentDropdown && (
                    <div className="absolute top-[calc(100%+6px)] left-1/2 -translate-x-1/2 min-w-[240px] bg-white border border-gray-200 rounded-xl shadow-xl max-h-80 overflow-y-auto z-[100]">
                      <div className="px-4 py-3 text-[11px] font-semibold text-gray-400 uppercase tracking-wide border-b border-gray-100">
                        Select Agent
                      </div>
                      {profiles.map((agent, idx) => (
                        <div
                          key={idx}
                          className="flex items-center gap-3 px-4 py-2.5 cursor-pointer transition-all hover:bg-gray-50 border-l-[3px] border-transparent hover:border-gray-800"
                          onClick={() => selectAgent(agent, idx)}
                        >
                          <div className="w-8 h-8 min-w-[32px] bg-gradient-to-br from-gray-800 to-gray-600 text-white rounded-full flex items-center justify-center text-xs font-semibold flex-shrink-0 shadow-sm">
                            {(agent.username || "A")[0]}
                          </div>
                          <div className="flex flex-col gap-0.5 flex-1 min-w-0">
                            <span className="text-[13px] font-semibold text-gray-800 truncate">
                              {agent.username}
                            </span>
                            <span className="text-[11px] text-gray-400 truncate">
                              {agent.profession || "Unknown"}
                            </span>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {/* Divider */}
              <div className="w-px h-6 bg-gray-200 mx-1.5" />

              {/* Survey tab */}
              <button
                className={`flex items-center gap-1.5 px-3.5 py-2 text-xs font-medium rounded-full transition-all whitespace-nowrap ${
                  activeTab === "survey"
                    ? "bg-emerald-700 text-white shadow-md"
                    : "bg-emerald-50 text-emerald-700 hover:bg-emerald-100"
                }`}
                onClick={selectSurveyTab}
              >
                <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2}>
                  <path d="M9 11l3 3L22 4" />
                  <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
                </svg>
                <span>Send Survey</span>
              </button>
            </div>
          </div>

          {/* ── Chat Mode ── */}
          {activeTab === "chat" && (
            <div className="flex-1 flex flex-col overflow-hidden">
              {/* Profile / Tools card */}
              {chatTarget === "report_agent" && <ReportAgentToolsCard />}
              {chatTarget === "agent" && selectedAgent && (
                <AgentProfileCard agent={selectedAgent} />
              )}

              {/* Messages area */}
              <div
                ref={chatMessagesRef}
                className="flex-1 overflow-y-auto px-6 py-5 flex flex-col gap-5"
              >
                {/* Empty state */}
                {chatHistory.length === 0 && (
                  <div className="flex-1 flex flex-col items-center justify-center gap-4 text-gray-400">
                    <div className="opacity-30">
                      <svg viewBox="0 0 24 24" width={48} height={48} fill="none" stroke="currentColor" strokeWidth={1.5}>
                        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
                      </svg>
                    </div>
                    <p className="text-sm text-center max-w-[280px] leading-relaxed">
                      {chatTarget === "report_agent"
                        ? "Chat with the Report Agent to explore the report in depth"
                        : "Chat with a simulated agent to understand their perspective"}
                    </p>
                  </div>
                )}

                {/* Message bubbles */}
                {chatHistory.map((msg, i) => (
                  <div key={i} className={`flex gap-3 ${msg.role === "user" ? "flex-row-reverse" : ""}`}>
                    {/* Avatar */}
                    <div
                      className={`w-9 h-9 min-w-[36px] rounded-full flex items-center justify-center text-sm font-semibold flex-shrink-0 ${
                        msg.role === "user"
                          ? "bg-gray-800 text-white"
                          : "bg-gray-100 text-gray-600"
                      }`}
                    >
                      {msg.role === "user"
                        ? "U"
                        : chatTarget === "report_agent"
                          ? "R"
                          : (selectedAgent?.username?.[0] || "A")}
                    </div>

                    {/* Content */}
                    <div className={`max-w-[70%] flex flex-col gap-1.5 ${msg.role === "user" ? "items-end" : ""}`}>
                      {/* Sender & time */}
                      <div className={`flex items-center gap-2 ${msg.role === "user" ? "flex-row-reverse" : ""}`}>
                        <span className="text-xs font-semibold text-gray-600">
                          {msg.role === "user"
                            ? "You"
                            : chatTarget === "report_agent"
                              ? "Report Agent"
                              : selectedAgent?.username || "Agent"}
                        </span>
                        <span className="text-[11px] text-gray-400">
                          {formatTime(msg.timestamp)}
                        </span>
                      </div>

                      {/* Message bubble */}
                      <div
                        className={`px-3.5 py-2.5 rounded-xl text-sm leading-relaxed ${
                          msg.role === "user"
                            ? "bg-gray-800 text-white rounded-br-sm"
                            : "bg-gray-100 text-gray-700 rounded-bl-sm"
                        }`}
                      >
                        {msg.role === "assistant" ? (
                          <div className="prose prose-sm max-w-none prose-p:text-gray-700 prose-p:my-1 prose-headings:text-gray-900">
                            <ReactMarkdown>{msg.content}</ReactMarkdown>
                          </div>
                        ) : (
                          <p className="whitespace-pre-wrap m-0">{msg.content}</p>
                        )}
                      </div>

                      {/* Sources */}
                      {msg.sources && msg.sources.length > 0 && (
                        <div className="mt-1 pt-1.5">
                          <span className="text-[10px] text-gray-400 font-mono uppercase">
                            Sources:
                          </span>
                          <div className="mt-1 space-y-1">
                            {msg.sources.map((src: any, j: number) => (
                              <div
                                key={j}
                                className="text-[11px] text-gray-500 bg-gray-50 rounded px-2 py-1 border border-gray-100"
                              >
                                {src.title || src.name || `Source ${j + 1}`}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </div>
                ))}

                {/* Typing indicator */}
                {isSending && (
                  <div className="flex gap-3">
                    <div className="w-9 h-9 min-w-[36px] bg-gray-100 text-gray-600 rounded-full flex items-center justify-center text-sm font-semibold flex-shrink-0">
                      {chatTarget === "report_agent" ? "R" : (selectedAgent?.username?.[0] || "A")}
                    </div>
                    <div className="bg-gray-100 rounded-xl rounded-bl-sm px-3.5 py-2.5">
                      <div className="flex gap-1">
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "0ms" }} />
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "150ms" }} />
                        <div className="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style={{ animationDelay: "300ms" }} />
                      </div>
                    </div>
                  </div>
                )}
              </div>

              {/* Chat input */}
              <div className="border-t border-gray-200 px-6 py-4 flex gap-3 items-end flex-shrink-0">
                <textarea
                  ref={chatInputRef}
                  value={chatInput}
                  onChange={(e) => setChatInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault()
                      sendMessage()
                    }
                  }}
                  placeholder={
                    chatTarget === "agent" && !selectedAgent
                      ? "Select an agent first..."
                      : "Ask a question..."
                  }
                  disabled={isSending || (chatTarget === "agent" && !selectedAgent)}
                  rows={1}
                  className="flex-1 px-4 py-3 text-sm border border-gray-200 rounded-lg resize-none focus:outline-none focus:border-gray-800 transition-colors disabled:bg-gray-50 disabled:cursor-not-allowed"
                />
                <button
                  className="w-11 h-11 bg-gray-800 text-white rounded-lg flex items-center justify-center hover:bg-gray-700 transition-colors disabled:bg-gray-200 disabled:text-gray-400 disabled:cursor-not-allowed flex-shrink-0"
                  onClick={sendMessage}
                  disabled={
                    !chatInput.trim() ||
                    isSending ||
                    (chatTarget === "agent" && !selectedAgent)
                  }
                >
                  <Send size={18} />
                </button>
              </div>
            </div>
          )}

          {/* ── Survey Mode ── */}
          {activeTab === "survey" && (
            <div className="flex-1 flex flex-col overflow-hidden">
              {/* Survey setup */}
              <div className="flex-1 flex flex-col p-6 border-b border-gray-200 overflow-hidden">
                {/* Agent selection */}
                <div className="mb-6 flex-1 flex flex-col overflow-hidden min-h-0">
                  <div className="flex justify-between items-center mb-3">
                    <span className="text-[13px] font-semibold text-gray-700">
                      Select Survey Targets
                    </span>
                    <span className="text-xs text-gray-400">
                      Selected {selectedAgents.size} / {profiles.length}
                    </span>
                  </div>

                  <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-2.5 flex-1 overflow-y-auto p-1 content-start">
                    {profiles.map((agent, idx) => {
                      const checked = selectedAgents.has(idx)
                      return (
                        <label
                          key={idx}
                          className={`flex items-center gap-2.5 px-3 py-2.5 rounded-lg border cursor-pointer transition-all ${
                            checked
                              ? "bg-green-50 border-emerald-500"
                              : "bg-gray-50 border-gray-200 hover:border-gray-300"
                          }`}
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggleAgentSelection(idx)}
                            className="hidden"
                          />
                          <div
                            className={`w-7 h-7 min-w-[28px] rounded-full flex items-center justify-center text-[11px] font-semibold flex-shrink-0 ${
                              checked
                                ? "bg-emerald-500 text-white"
                                : "bg-gray-200 text-gray-600"
                            }`}
                          >
                            {(agent.username || "A")[0]}
                          </div>
                          <div className="flex-1 min-w-0">
                            <span className="block text-xs font-semibold text-gray-800 truncate">
                              {agent.username}
                            </span>
                            <span className="block text-[10px] text-gray-400 truncate">
                              {agent.profession || "Unknown"}
                            </span>
                          </div>
                          <div
                            className={`w-5 h-5 rounded border-2 flex items-center justify-center flex-shrink-0 transition-all ${
                              checked
                                ? "bg-emerald-500 border-emerald-500"
                                : "border-gray-200"
                            }`}
                          >
                            {checked && (
                              <svg viewBox="0 0 24 24" width={16} height={16} fill="none" stroke="white" strokeWidth={3}>
                                <polyline points="20 6 9 17 4 12" />
                              </svg>
                            )}
                          </div>
                        </label>
                      )
                    })}
                  </div>

                  <div className="flex gap-2 mt-3">
                    <button
                      className="text-xs text-gray-500 hover:text-gray-800 hover:underline"
                      onClick={selectAllAgents}
                    >
                      Select All
                    </button>
                    <span className="text-gray-200">|</span>
                    <button
                      className="text-xs text-gray-500 hover:text-gray-800 hover:underline"
                      onClick={clearAgentSelection}
                    >
                      Clear
                    </button>
                  </div>
                </div>

                {/* Question input */}
                <div className="mb-5">
                  <div className="text-[13px] font-semibold text-gray-700 mb-3">
                    Survey Question
                  </div>
                  <textarea
                    value={surveyQuestion}
                    onChange={(e) => setSurveyQuestion(e.target.value)}
                    placeholder="Enter the question you want to ask all selected agents..."
                    rows={3}
                    className="w-full px-4 py-3.5 text-sm border border-gray-200 rounded-lg resize-none focus:outline-none focus:border-gray-800 transition-colors"
                  />
                </div>

                {/* Submit button */}
                <Button
                  className="w-full"
                  disabled={selectedAgents.size === 0 || !surveyQuestion.trim() || isSurveying}
                  onClick={submitSurvey}
                >
                  {isSurveying ? (
                    <>
                      <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2" />
                      Sending...
                    </>
                  ) : (
                    "Send Survey"
                  )}
                </Button>
              </div>

              {/* Survey results */}
              {surveyResults.length > 0 && (
                <div className="flex-1 overflow-y-auto p-6">
                  <div className="flex justify-between items-center mb-5">
                    <span className="text-sm font-semibold text-gray-800">Survey Results</span>
                    <span className="text-xs text-gray-400">{surveyResults.length} responses</span>
                  </div>
                  <div className="flex flex-col gap-4">
                    {surveyResults.map((result, idx) => (
                      <div
                        key={idx}
                        className="bg-gray-50 border border-gray-200 rounded-xl p-5"
                      >
                        {/* Result header */}
                        <div className="flex items-center gap-3 mb-3">
                          <div className="w-9 h-9 min-w-[36px] bg-gray-800 text-white rounded-full flex items-center justify-center text-sm font-semibold flex-shrink-0">
                            {(result.agent_name || "A")[0]}
                          </div>
                          <div className="flex flex-col gap-0.5">
                            <span className="text-sm font-semibold text-gray-800">
                              {result.agent_name}
                            </span>
                            <span className="text-xs text-gray-400">
                              {result.profession || "Unknown"}
                            </span>
                          </div>
                        </div>

                        {/* Question */}
                        <div className="flex items-start gap-2 p-3 bg-white rounded-lg mb-3 text-[13px] text-gray-500">
                          <svg viewBox="0 0 24 24" width={14} height={14} fill="none" stroke="currentColor" strokeWidth={2} className="flex-shrink-0 mt-0.5">
                            <circle cx="12" cy="12" r="10" />
                            <path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3" />
                            <line x1="12" y1="17" x2="12.01" y2="17" />
                          </svg>
                          <span>{result.question}</span>
                        </div>

                        {/* Answer */}
                        <div className="text-sm text-gray-700 leading-relaxed">
                          <ReactMarkdown>{result.answer}</ReactMarkdown>
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
