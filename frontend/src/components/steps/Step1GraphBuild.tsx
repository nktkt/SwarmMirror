"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useMemo, useRef, useEffect, useState, useCallback } from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { createSimulation } from "@/lib/api/simulation"

/* ------------------------------------------------------------------ */
/*  Types                                                              */
/* ------------------------------------------------------------------ */

interface OntologyAttribute {
  name: string
  type: string
  description: string
}

interface OntologyEntity {
  name: string
  description?: string
  attributes?: OntologyAttribute[]
  examples?: string[]
}

interface OntologyRelation {
  name: string
  description?: string
  attributes?: OntologyAttribute[]
  source_targets?: { source: string; target: string }[]
}

interface SelectedOntologyItem {
  itemType: "entity" | "relation"
  name: string
  description?: string
  attributes?: OntologyAttribute[]
  examples?: string[]
  source_targets?: { source: string; target: string }[]
}

interface Step1Props {
  currentPhase: number
  projectData: any
  ontologyProgress: any
  buildProgress: any
  graphData: any
  systemLogs: { time: string; msg: string }[]
  onNextStep: (params?: any) => void
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function StepBadge({ phase, current, labels }: { phase: number; current: number; labels?: { complete?: string; active?: string; pending?: string } }) {
  const complete = labels?.complete ?? "Complete"
  const active = labels?.active ?? "Generating"
  const pending = labels?.pending ?? "Waiting"

  if (current > phase) return <Badge variant="success">{complete}</Badge>
  if (current === phase) return <Badge variant="processing">{active}</Badge>
  return <Badge variant="secondary">{pending}</Badge>
}

function Spinner({ className = "" }: { className?: string }) {
  return (
    <div
      className={`border-2 border-orange-200 border-t-primary rounded-full animate-spin ${className}`}
      style={{ width: 14, height: 14 }}
    />
  )
}

/* ------------------------------------------------------------------ */
/*  Component                                                          */
/* ------------------------------------------------------------------ */

export default function Step1GraphBuild({
  currentPhase,
  projectData,
  ontologyProgress,
  buildProgress,
  graphData,
  systemLogs,
}: Step1Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)

  /* ---------- local state ---------- */
  const [creatingSimulation, setCreatingSimulation] = useState(false)
  const [selectedOntologyItem, setSelectedOntologyItem] = useState<SelectedOntologyItem | null>(null)
  const [createError, setCreateError] = useState<string | null>(null)

  /* ---------- derived ---------- */
  const graphStats = useMemo(() => {
    const nodes = graphData?.node_count || graphData?.nodes?.length || 0
    const edges = graphData?.edge_count || graphData?.edges?.length || 0
    const types = projectData?.ontology?.entity_types?.length || 0
    return { nodes, edges, types }
  }, [graphData, projectData])

  const entityTypes: OntologyEntity[] = projectData?.ontology?.entity_types ?? []
  const edgeTypes: OntologyRelation[] = projectData?.ontology?.edge_types ?? []

  /* ---------- effects ---------- */
  // Auto-scroll system logs
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight
    }
  }, [systemLogs])

  /* ---------- handlers ---------- */
  const selectOntologyItem = useCallback((item: OntologyEntity | OntologyRelation, type: "entity" | "relation") => {
    setSelectedOntologyItem({ ...item, itemType: type } as SelectedOntologyItem)
  }, [])

  const handleCloseOverlay = useCallback(() => {
    setSelectedOntologyItem(null)
  }, [])

  const handleEnterEnvSetup = useCallback(async () => {
    if (!projectData?.project_id || !projectData?.graph_id) {
      setCreateError("Missing project or graph information.")
      return
    }
    setCreatingSimulation(true)
    setCreateError(null)

    try {
      const res: any = await createSimulation({
        project_id: projectData.project_id,
        graph_id: projectData.graph_id,
        enable_twitter: true,
        enable_reddit: true,
      })
      if (res.success && res.data?.simulation_id) {
        router.push(`/simulation/${res.data.simulation_id}`)
      } else {
        const msg = res.error || "Unknown error"
        setCreateError(`Failed to create simulation: ${msg}`)
      }
    } catch (err: any) {
      setCreateError(`Error creating simulation: ${err.message}`)
    } finally {
      setCreatingSimulation(false)
    }
  }, [projectData, router])

  /* ---------- sub-renders ---------- */

  /** Ontology detail overlay shown when an entity/relation tag is clicked */
  const renderDetailOverlay = () => {
    if (!selectedOntologyItem) return null
    return (
      <div className="absolute inset-x-5 top-[60px] bottom-5 bg-white/[.98] backdrop-blur-sm border border-gray-200 rounded-md shadow-lg z-10 flex flex-col overflow-hidden animate-in fade-in slide-in-from-bottom-1 duration-200">
        {/* Header */}
        <div className="flex justify-between items-center px-4 py-3 border-b border-gray-200 bg-gray-50/80">
          <div className="flex items-center gap-2">
            <span className="text-[9px] font-bold text-white bg-black px-1.5 py-0.5 rounded uppercase tracking-wider">
              {selectedOntologyItem.itemType === "entity" ? "ENTITY" : "RELATION"}
            </span>
            <span className="text-sm font-bold font-mono">{selectedOntologyItem.name}</span>
          </div>
          <button
            className="text-gray-400 hover:text-gray-700 text-lg leading-none transition-colors"
            onClick={handleCloseOverlay}
            aria-label="Close overlay"
          >
            &times;
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-4">
          {/* Description */}
          {selectedOntologyItem.description && (
            <p className="text-xs text-gray-600 leading-relaxed mb-4 pb-3 border-b border-dashed border-gray-200">
              {selectedOntologyItem.description}
            </p>
          )}

          {/* Attributes */}
          {selectedOntologyItem.attributes && selectedOntologyItem.attributes.length > 0 && (
            <div className="mb-4">
              <span className="text-[10px] font-semibold text-gray-400 block mb-2 uppercase tracking-wide">
                ATTRIBUTES
              </span>
              <div className="flex flex-col gap-1.5">
                {selectedOntologyItem.attributes.map((attr) => (
                  <div
                    key={attr.name}
                    className="text-[11px] flex flex-wrap gap-1.5 items-baseline bg-gray-50 p-1.5 rounded"
                  >
                    <span className="font-mono font-semibold text-black">{attr.name}</span>
                    <span className="text-gray-400 text-[10px]">({attr.type})</span>
                    <span className="text-gray-600 flex-1 min-w-[150px]">{attr.description}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Examples (entity only) */}
          {selectedOntologyItem.examples && selectedOntologyItem.examples.length > 0 && (
            <div className="mb-4">
              <span className="text-[10px] font-semibold text-gray-400 block mb-2 uppercase tracking-wide">
                EXAMPLES
              </span>
              <div className="flex flex-wrap gap-1.5">
                {selectedOntologyItem.examples.map((ex) => (
                  <span
                    key={ex}
                    className="text-[11px] bg-white border border-gray-200 px-2 py-0.5 rounded-full text-gray-600"
                  >
                    {ex}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Connections / source_targets (relation only) */}
          {selectedOntologyItem.source_targets && selectedOntologyItem.source_targets.length > 0 && (
            <div className="mb-4">
              <span className="text-[10px] font-semibold text-gray-400 block mb-2 uppercase tracking-wide">
                CONNECTIONS
              </span>
              <div className="flex flex-col gap-1.5">
                {selectedOntologyItem.source_targets.map((conn, idx) => (
                  <div
                    key={idx}
                    className="flex items-center gap-2 text-[11px] p-1.5 bg-gray-50 rounded font-mono"
                  >
                    <span className="font-semibold text-gray-800">{conn.source}</span>
                    <span className="text-gray-400">&rarr;</span>
                    <span className="font-semibold text-gray-800">{conn.target}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    )
  }

  /* ------------------------------------------------------------------ */
  /*  Render                                                             */
  /* ------------------------------------------------------------------ */
  return (
    <div className="h-full bg-[#FAFAFA] flex flex-col overflow-hidden">
      {/* ===== Scrollable card stack ===== */}
      <div className="flex-1 overflow-y-auto p-6 flex flex-col gap-5">
        {/* ── Step 01: Ontology Generation ──────────────────────────── */}
        <div
          className={`bg-white rounded-lg p-5 shadow-sm border transition-all relative ${
            currentPhase === 0
              ? "border-primary shadow-md"
              : currentPhase > 0
                ? "border-gray-200"
                : "border-gray-200"
          }`}
        >
          {/* Card header */}
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span
                className={`font-mono text-xl font-bold ${
                  currentPhase >= 0 ? "text-black" : "text-gray-300"
                }`}
              >
                01
              </span>
              <span className="font-semibold text-sm tracking-wide">Ontology Generation</span>
            </div>
            <StepBadge phase={0} current={currentPhase} labels={{ active: "Generating" }} />
          </div>

          {/* API note & description */}
          <p className="font-mono text-[10px] text-gray-400 mb-2">POST /api/graph/ontology/generate</p>
          <p className="text-xs text-gray-500 leading-relaxed mb-4">
            LLM analyzes document content and simulation requirements, extracts reality seeds, and
            auto-generates the ontology structure.
          </p>

          {/* Progress spinner during generation */}
          {currentPhase === 0 && ontologyProgress && (
            <div className="flex items-center gap-2.5 text-xs text-primary mb-3">
              <Spinner />
              <span>{ontologyProgress.message || "Analyzing documents..."}</span>
            </div>
          )}

          {/* ── Detail overlay (absolute positioned within this card) ── */}
          {renderDetailOverlay()}

          {/* ── Entity type tags ── */}
          {entityTypes.length > 0 && (
            <div
              className={`mt-3 transition-opacity duration-300 ${
                selectedOntologyItem ? "opacity-30 pointer-events-none" : ""
              }`}
            >
              <span className="text-[10px] text-gray-400 font-semibold block mb-2 uppercase tracking-wide">
                GENERATED ENTITY TYPES
              </span>
              <div className="flex flex-wrap gap-2">
                {entityTypes.map((entity) => (
                  <span
                    key={entity.name}
                    className="bg-gray-100 border border-gray-200 px-2.5 py-1 rounded text-[11px] font-mono text-gray-700 cursor-pointer hover:bg-gray-200 hover:border-gray-300 transition-colors"
                    onClick={() => selectOntologyItem(entity, "entity")}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => e.key === "Enter" && selectOntologyItem(entity, "entity")}
                  >
                    {entity.name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* ── Relation type tags ── */}
          {edgeTypes.length > 0 && (
            <div
              className={`mt-3 transition-opacity duration-300 ${
                selectedOntologyItem ? "opacity-30 pointer-events-none" : ""
              }`}
            >
              <span className="text-[10px] text-gray-400 font-semibold block mb-2 uppercase tracking-wide">
                GENERATED RELATION TYPES
              </span>
              <div className="flex flex-wrap gap-2">
                {edgeTypes.map((rel) => (
                  <span
                    key={rel.name}
                    className="bg-gray-100 border border-gray-200 px-2.5 py-1 rounded text-[11px] font-mono text-gray-700 cursor-pointer hover:bg-gray-200 hover:border-gray-300 transition-colors"
                    onClick={() => selectOntologyItem(rel, "relation")}
                    role="button"
                    tabIndex={0}
                    onKeyDown={(e) => e.key === "Enter" && selectOntologyItem(rel, "relation")}
                  >
                    {rel.name}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* ── Step 02: GraphRAG Build ───────────────────────────────── */}
        <div
          className={`bg-white rounded-lg p-5 shadow-sm border transition-all ${
            currentPhase === 1
              ? "border-primary shadow-md"
              : currentPhase > 1
                ? "border-gray-200"
                : "border-gray-200"
          }`}
        >
          {/* Card header */}
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span
                className={`font-mono text-xl font-bold ${
                  currentPhase >= 1 ? "text-black" : "text-gray-300"
                }`}
              >
                02
              </span>
              <span className="font-semibold text-sm tracking-wide">GraphRAG Build</span>
            </div>
            <StepBadge
              phase={1}
              current={currentPhase}
              labels={{ active: `${buildProgress?.progress || 0}%` }}
            />
          </div>

          {/* API note & description */}
          <p className="font-mono text-[10px] text-gray-400 mb-2">POST /api/graph/build</p>
          <p className="text-xs text-gray-500 leading-relaxed mb-4">
            Build knowledge graph from the generated ontology using Zep, extracting entities and
            relations with temporal memory and community summaries.
          </p>

          {/* Build progress bar (visible during building) */}
          {currentPhase === 1 && buildProgress && (
            <div className="mb-4">
              <div className="flex justify-between text-[10px] text-gray-500 mb-1.5">
                <span className="font-mono">{buildProgress.status || "Building..."}</span>
                <span className="font-mono font-semibold">{buildProgress.progress || 0}%</span>
              </div>
              <div className="w-full h-1.5 bg-gray-100 rounded-full overflow-hidden">
                <div
                  className="h-full bg-primary rounded-full transition-all duration-500 ease-out"
                  style={{ width: `${buildProgress.progress || 0}%` }}
                />
              </div>
              {buildProgress.chunks_processed != null && (
                <div className="text-[10px] text-gray-400 mt-1 font-mono">
                  Chunks: {buildProgress.chunks_processed}
                  {buildProgress.total_chunks ? ` / ${buildProgress.total_chunks}` : ""}
                </div>
              )}
            </div>
          )}

          {/* Stats grid */}
          <div className="grid grid-cols-3 gap-3 bg-gray-50 p-4 rounded-md">
            {([
              { label: "Entity Nodes", value: graphStats.nodes },
              { label: "Relation Edges", value: graphStats.edges },
              { label: "Schema Types", value: graphStats.types },
            ] as const).map((stat) => (
              <div key={stat.label} className="text-center">
                <span className="block text-xl font-bold font-mono">{stat.value}</span>
                <span className="text-[9px] text-gray-400 uppercase mt-1 block tracking-wide">
                  {stat.label}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* ── Step 03: Build Complete ───────────────────────────────── */}
        <div
          className={`bg-white rounded-lg p-5 shadow-sm border transition-all ${
            currentPhase === 2
              ? "border-primary shadow-md"
              : currentPhase > 2
                ? "border-gray-200"
                : "border-gray-200"
          }`}
        >
          {/* Card header */}
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span
                className={`font-mono text-xl font-bold ${
                  currentPhase >= 2 ? "text-black" : "text-gray-300"
                }`}
              >
                03
              </span>
              <span className="font-semibold text-sm tracking-wide">Build Complete</span>
            </div>
            {currentPhase >= 2 && <Badge variant="processing">Ready</Badge>}
          </div>

          {/* API note & description */}
          <p className="font-mono text-[10px] text-gray-400 mb-2">POST /api/simulation/create</p>
          <p className="text-xs text-gray-500 leading-relaxed mb-4">
            Graph build complete. Proceed to environment setup for simulation.
          </p>

          {/* Error display */}
          {createError && (
            <div className="mb-3 p-3 bg-red-50 border border-red-200 rounded text-xs text-red-700">
              {createError}
            </div>
          )}

          {/* Action button */}
          <Button
            className="w-full rounded"
            disabled={currentPhase < 2 || creatingSimulation}
            onClick={handleEnterEnvSetup}
          >
            {creatingSimulation && (
              <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin mr-2" />
            )}
            {creatingSimulation ? "Creating..." : "Enter Environment Setup"}
            {!creatingSimulation && <span className="ml-1">&rarr;</span>}
          </Button>
        </div>
      </div>

      {/* ===== System Logs (pinned to bottom) ===== */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span className="uppercase tracking-wider">SYSTEM DASHBOARD</span>
          <span>{projectData?.project_id || "NO_PROJECT"}</span>
        </div>
        <div
          ref={logRef}
          className="flex flex-col gap-1 h-20 overflow-y-auto pr-1 scrollbar-thin"
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
