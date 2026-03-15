"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState, useEffect, useCallback } from "react"
import { useRouter } from "next/navigation"
import { getSimulationHistory } from "@/lib/api/simulation"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"

function formatSimulationId(id: string) {
  if (!id) return "SIM_UNKNOWN"
  const prefix = id.replace("sim_", "").slice(0, 6)
  return `SIM_${prefix.toUpperCase()}`
}

function formatDate(dateStr: string) {
  if (!dateStr) return ""
  try {
    return new Date(dateStr).toISOString().slice(0, 10)
  } catch {
    return dateStr?.slice(0, 10) || ""
  }
}

function formatTime(dateStr: string) {
  if (!dateStr) return ""
  try {
    const d = new Date(dateStr)
    return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`
  } catch {
    return ""
  }
}

function formatRounds(sim: any) {
  const current = sim.current_round || 0
  const total = sim.total_rounds || 0
  if (total === 0) return "Not started"
  return `${current}/${total} rounds`
}

function getProgressClass(sim: any) {
  const current = sim.current_round || 0
  const total = sim.total_rounds || 0
  if (total === 0 || current === 0) return "text-gray-400"
  if (current >= total) return "text-green-500"
  return "text-amber-500"
}

function getFileTypeLabel(filename: string) {
  if (!filename) return "FILE"
  return filename.split(".").pop()?.toUpperCase() || "FILE"
}

function truncateText(text: string, max: number) {
  if (!text) return ""
  return text.length > max ? text.slice(0, max) + "..." : text
}

export default function HistoryPanel() {
  const router = useRouter()
  const [projects, setProjects] = useState<any[]>([])
  const [loading, setLoading] = useState(true)
  const [selected, setSelected] = useState<any>(null)
  const [viewMode, setViewMode] = useState<"grid" | "stack">("grid")
  const [expandedCards, setExpandedCards] = useState<Set<string>>(new Set())

  const toggleCardExpand = (id: string) => {
    setExpandedCards(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const loadHistory = useCallback(async () => {
    try {
      setLoading(true)
      const res: any = await getSimulationHistory(20)
      if (res.success) {
        setProjects(res.data || [])
      }
    } catch {
      setProjects([])
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadHistory()
  }, [loadHistory])

  if (loading) {
    return (
      <div className="mt-10 text-center py-12">
        <div className="inline-block w-6 h-6 border-2 border-gray-200 border-t-gray-600 rounded-full animate-spin" />
        <p className="mt-3 text-sm text-gray-400 font-mono tracking-wider">Loading...</p>
      </div>
    )
  }

  if (projects.length === 0) {
    return (
      <div className="mt-10 text-center py-8">
        <div className="flex items-center justify-center gap-6 mb-4">
          <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent max-w-[300px]" />
          <span className="text-xs font-mono text-gray-400 tracking-[3px] uppercase">History</span>
          <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent max-w-[300px]" />
        </div>
      </div>
    )
  }

  return (
    <div className="mt-10 relative">
      {/* Background grid */}
      <div className="absolute inset-0 pointer-events-none overflow-hidden">
        <div
          className="absolute inset-0"
          style={{
            backgroundImage:
              "linear-gradient(to right, rgba(0,0,0,0.05) 1px, transparent 1px), linear-gradient(to bottom, rgba(0,0,0,0.05) 1px, transparent 1px)",
            backgroundSize: "50px 50px",
          }}
        />
        <div className="absolute inset-0 bg-gradient-to-r from-white/90 via-transparent to-white/90" />
      </div>

      {/* Section Header */}
      <div className="relative z-10 flex items-center justify-center gap-6 mb-6">
        <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent max-w-[300px]" />
        <span className="text-xs font-mono text-gray-400 tracking-[3px] uppercase">History</span>
        <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent max-w-[300px]" />
        <div className="flex gap-1">
          <button onClick={() => setViewMode("grid")} className={`px-2 py-1 text-[10px] font-mono rounded transition-colors ${viewMode === "grid" ? "bg-gray-800 text-white" : "bg-gray-100 text-gray-500 hover:bg-gray-200"}`}>Grid</button>
          <button onClick={() => setViewMode("stack")} className={`px-2 py-1 text-[10px] font-mono rounded transition-colors ${viewMode === "stack" ? "bg-gray-800 text-white" : "bg-gray-100 text-gray-500 hover:bg-gray-200"}`}>Stack</button>
        </div>
      </div>

      {/* Cards */}
      <div className={`relative z-10 px-4 ${viewMode === "grid" ? "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6" : "flex flex-col items-center gap-3 max-w-xl mx-auto"}`}>
        {projects.map((project: any, idx: number) => {
          const isExpanded = expandedCards.has(project.simulation_id)
          const stackStyle = viewMode === "stack" && !isExpanded ? {
            transform: `perspective(800px) rotateX(${Math.min(idx * 1.5, 6)}deg)`,
            zIndex: projects.length - idx,
            opacity: Math.max(1 - idx * 0.06, 0.5),
          } : {}

          return (
          <div
            key={project.simulation_id}
            className={`bg-white border border-gray-200 p-4 cursor-pointer transition-all hover:shadow-lg hover:border-gray-400 group ${viewMode === "stack" ? "w-full" : ""} ${isExpanded ? "ring-2 ring-blue-300" : ""}`}
            style={stackStyle}
            onClick={(e) => {
              if (viewMode === "stack") {
                e.stopPropagation()
                toggleCardExpand(project.simulation_id)
              } else {
                setSelected(project)
              }
            }}
            onDoubleClick={() => setSelected(project)}
          >
            {/* Header */}
            <div className="flex justify-between items-center mb-3 pb-3 border-b border-gray-100 font-mono text-[11px]">
              <span className="text-gray-500 tracking-wide">{formatSimulationId(project.simulation_id)}</span>
              <div className="flex gap-1.5">
                <span className={project.project_id ? "text-blue-500" : "text-gray-300 opacity-50"}>&#9671;</span>
                <span className="text-amber-500">&#9672;</span>
                <span className={project.report_id ? "text-green-500" : "text-gray-300 opacity-50"}>&#9670;</span>
              </div>
            </div>

            {/* Files */}
            <div className="relative bg-gradient-to-br from-gray-50 to-gray-100 rounded border border-gray-200 p-2 mb-3 min-h-[48px]">
              {project.files?.length > 0 ? (
                <div className="flex flex-col gap-1">
                  {project.files.slice(0, 3).map((file: any, i: number) => (
                    <div key={i} className="flex items-center gap-2 bg-white/70 rounded px-2 py-1">
                      <span className="text-[10px] font-mono font-semibold px-1 rounded bg-gray-100 text-gray-600 uppercase">
                        {getFileTypeLabel(file.filename)}
                      </span>
                      <span className="text-[11px] text-gray-600 truncate">{file.filename}</span>
                    </div>
                  ))}
                  {project.files.length > 3 && (
                    <span className="text-[10px] text-gray-400 text-center font-mono">+{project.files.length - 3} files</span>
                  )}
                </div>
              ) : (
                <div className="flex items-center justify-center h-12 text-gray-400 text-xs font-mono">No files</div>
              )}
            </div>

            {/* Title */}
            <h3 className="text-sm font-bold text-gray-900 mb-1 truncate group-hover:text-blue-600 transition-colors">
              {truncateText(project.simulation_requirement, 20) || "Unnamed simulation"}
            </h3>
            <p className="text-xs text-gray-500 mb-4 line-clamp-2 h-[34px]">
              {truncateText(project.simulation_requirement, 55)}
            </p>

            {/* Footer */}
            <div className="flex justify-between items-center pt-3 border-t border-gray-100 font-mono text-[10px] text-gray-400">
              <div className="flex items-center gap-2">
                <span>{formatDate(project.created_at)}</span>
                <span>{formatTime(project.created_at)}</span>
              </div>
              <span className={`font-semibold ${getProgressClass(project)}`}>
                {formatRounds(project)}
              </span>
            </div>
          </div>
          )
        })}
      </div>

      {/* Detail Modal */}
      <Dialog open={!!selected} onOpenChange={(open) => !open && setSelected(null)}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-4">
              <span className="font-mono text-base">{selected && formatSimulationId(selected.simulation_id)}</span>
              <span className={`text-xs font-mono font-semibold ${selected && getProgressClass(selected)}`}>
                {selected && formatRounds(selected)}
              </span>
            </DialogTitle>
            <DialogDescription className="font-mono text-xs">
              {selected && `${formatDate(selected.created_at)} ${formatTime(selected.created_at)}`}
            </DialogDescription>
          </DialogHeader>

          {selected && (
            <div className="space-y-4">
              <div>
                <p className="text-xs text-gray-500 font-mono uppercase tracking-wider mb-2">Simulation Requirement</p>
                <div className="bg-gray-50 border border-gray-100 rounded-lg p-4 text-sm text-gray-700 leading-relaxed">
                  {selected.simulation_requirement || "None"}
                </div>
              </div>

              {selected.files?.length > 0 && (
                <div>
                  <p className="text-xs text-gray-500 font-mono uppercase tracking-wider mb-2">Files</p>
                  <div className="space-y-2 max-h-[200px] overflow-y-auto">
                    {selected.files.map((file: any, i: number) => (
                      <div key={i} className="flex items-center gap-3 bg-white border border-gray-200 rounded px-3 py-2">
                        <span className="text-[10px] font-mono font-semibold px-1.5 py-0.5 rounded bg-gray-100 text-gray-600 uppercase">
                          {getFileTypeLabel(file.filename)}
                        </span>
                        <span className="text-sm text-gray-600 truncate">{file.filename}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex items-center gap-4 pt-2">
                <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent" />
                <span className="text-[10px] font-mono text-gray-400 tracking-widest uppercase">Playback</span>
                <div className="flex-1 h-px bg-gradient-to-r from-transparent via-gray-200 to-transparent" />
              </div>

              <div className="grid grid-cols-3 gap-3">
                <Button
                  variant="outline"
                  className="flex flex-col items-center gap-1 py-4 h-auto"
                  disabled={!selected.project_id}
                  onClick={() => {
                    router.push(`/process/${selected.project_id}`)
                    setSelected(null)
                  }}
                >
                  <span className="text-[10px] font-mono text-gray-400 uppercase">Step1</span>
                  <span className="text-blue-500">&#9671;</span>
                  <span className="text-xs font-semibold">Graph Build</span>
                </Button>
                <Button
                  variant="outline"
                  className="flex flex-col items-center gap-1 py-4 h-auto"
                  onClick={() => {
                    router.push(`/simulation/${selected.simulation_id}`)
                    setSelected(null)
                  }}
                >
                  <span className="text-[10px] font-mono text-gray-400 uppercase">Step2</span>
                  <span className="text-amber-500">&#9672;</span>
                  <span className="text-xs font-semibold">Env Setup</span>
                </Button>
                <Button
                  variant="outline"
                  className="flex flex-col items-center gap-1 py-4 h-auto"
                  disabled={!selected.report_id}
                  onClick={() => {
                    router.push(`/report/${selected.report_id}`)
                    setSelected(null)
                  }}
                >
                  <span className="text-[10px] font-mono text-gray-400 uppercase">Step4</span>
                  <span className="text-green-500">&#9670;</span>
                  <span className="text-xs font-semibold">Report</span>
                </Button>
              </div>

              <p className="text-[11px] text-gray-400 font-mono text-center">
                Step3 &quot;Simulation&quot; and Step5 &quot;Interaction&quot; require a running session and cannot be replayed.
              </p>
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
