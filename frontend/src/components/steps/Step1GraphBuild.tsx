"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useMemo, useRef, useEffect, useState } from "react"
import { useRouter } from "next/navigation"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { createSimulation } from "@/lib/api/simulation"

interface Step1Props {
  currentPhase: number
  projectData: any
  ontologyProgress: any
  buildProgress: any
  graphData: any
  systemLogs: { time: string; msg: string }[]
  onNextStep: (params?: any) => void
}

export default function Step1GraphBuild({ currentPhase, projectData, ontologyProgress, buildProgress, graphData, systemLogs, onNextStep }: Step1Props) {
  const router = useRouter()
  const logRef = useRef<HTMLDivElement>(null)
  const [creatingSimulation, setCreatingSimulation] = useState(false)
  const [selectedOntologyItem, setSelectedOntologyItem] = useState<any>(null)

  const graphStats = useMemo(() => {
    const nodes = graphData?.node_count || graphData?.nodes?.length || 0
    const edges = graphData?.edge_count || graphData?.edges?.length || 0
    const types = projectData?.ontology?.entity_types?.length || 0
    return { nodes, edges, types }
  }, [graphData, projectData])

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight
  }, [systemLogs])

  const handleEnterEnvSetup = async () => {
    if (!projectData?.project_id || !projectData?.graph_id) return
    setCreatingSimulation(true)
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
        alert("Failed to create simulation: " + (res.error || "Unknown error"))
      }
    } catch (err: any) {
      alert("Error creating simulation: " + err.message)
    } finally {
      setCreatingSimulation(false)
    }
  }

  return (
    <div className="h-full bg-[#FAFAFA] flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto p-6 space-y-5">
        {/* Step 01: Ontology */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border transition-all ${currentPhase === 0 ? "border-primary shadow-md" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${currentPhase >= 0 ? "text-black" : "text-gray-300"}`}>01</span>
              <span className="font-semibold text-sm">Ontology Generation</span>
            </div>
            {currentPhase > 0 ? <Badge variant="success">Complete</Badge>
              : currentPhase === 0 ? <Badge variant="processing">Generating</Badge>
              : <Badge variant="secondary">Waiting</Badge>}
          </div>
          <div className="text-xs text-gray-400 font-mono mb-2">POST /api/graph/ontology/generate</div>
          <p className="text-xs text-gray-500 leading-relaxed mb-4">
            LLM analyzes document content and simulation requirements, extracts reality seeds, and auto-generates the ontology structure.
          </p>

          {currentPhase === 0 && ontologyProgress && (
            <div className="flex items-center gap-2.5 text-xs text-primary mb-3">
              <div className="w-3.5 h-3.5 border-2 border-blue-200 border-t-primary rounded-full animate-spin" />
              <span>{ontologyProgress.message || "Analyzing documents..."}</span>
            </div>
          )}

          {/* Ontology Detail Overlay */}
          {selectedOntologyItem && (
            <div className="bg-white/98 border border-gray-200 rounded-md p-4 mb-3 animate-in fade-in duration-200">
              <div className="flex justify-between items-center mb-3">
                <div className="flex items-center gap-2">
                  <span className="text-[9px] font-bold text-white bg-black px-1.5 py-0.5 rounded uppercase">
                    {selectedOntologyItem.itemType}
                  </span>
                  <span className="text-sm font-bold font-mono">{selectedOntologyItem.name}</span>
                </div>
                <button className="text-gray-400 hover:text-gray-700" onClick={() => setSelectedOntologyItem(null)}>&times;</button>
              </div>
              <p className="text-xs text-gray-600 leading-relaxed mb-3 pb-3 border-b border-dashed border-gray-200">
                {selectedOntologyItem.description}
              </p>
              {selectedOntologyItem.attributes?.length > 0 && (
                <div className="mb-3">
                  <span className="text-[10px] font-semibold text-gray-400 block mb-2">ATTRIBUTES</span>
                  <div className="space-y-1.5">
                    {selectedOntologyItem.attributes.map((attr: any) => (
                      <div key={attr.name} className="text-[11px] flex flex-wrap gap-1.5 bg-gray-50 p-1 rounded">
                        <span className="font-mono font-semibold">{attr.name}</span>
                        <span className="text-gray-400 text-[10px]">({attr.type})</span>
                        <span className="text-gray-600">{attr.description}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
              {selectedOntologyItem.examples?.length > 0 && (
                <div>
                  <span className="text-[10px] font-semibold text-gray-400 block mb-2">EXAMPLES</span>
                  <div className="flex flex-wrap gap-1.5">
                    {selectedOntologyItem.examples.map((ex: string) => (
                      <span key={ex} className="text-[11px] bg-white border border-gray-200 px-2 py-0.5 rounded-full text-gray-600">{ex}</span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {projectData?.ontology?.entity_types && (
            <div className={`mt-3 ${selectedOntologyItem ? "opacity-30 pointer-events-none" : ""}`}>
              <span className="text-[10px] text-gray-400 font-semibold block mb-2">GENERATED ENTITY TYPES</span>
              <div className="flex flex-wrap gap-2">
                {projectData.ontology.entity_types.map((entity: any) => (
                  <span
                    key={entity.name}
                    className="bg-gray-100 border border-gray-200 px-2.5 py-1 rounded text-[11px] font-mono text-gray-700 cursor-pointer hover:bg-gray-200 transition-colors"
                    onClick={() => setSelectedOntologyItem({ ...entity, itemType: "entity" })}
                  >
                    {entity.name}
                  </span>
                ))}
              </div>
            </div>
          )}

          {projectData?.ontology?.edge_types && (
            <div className={`mt-3 ${selectedOntologyItem ? "opacity-30 pointer-events-none" : ""}`}>
              <span className="text-[10px] text-gray-400 font-semibold block mb-2">GENERATED RELATION TYPES</span>
              <div className="flex flex-wrap gap-2">
                {projectData.ontology.edge_types.map((rel: any) => (
                  <span
                    key={rel.name}
                    className="bg-gray-100 border border-gray-200 px-2.5 py-1 rounded text-[11px] font-mono text-gray-700 cursor-pointer hover:bg-gray-200 transition-colors"
                    onClick={() => setSelectedOntologyItem({ ...rel, itemType: "relation" })}
                  >
                    {rel.name}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>

        {/* Step 02: Graph Build */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border transition-all ${currentPhase === 1 ? "border-primary shadow-md" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${currentPhase >= 1 ? "text-black" : "text-gray-300"}`}>02</span>
              <span className="font-semibold text-sm">GraphRAG Build</span>
            </div>
            {currentPhase > 1 ? <Badge variant="success">Complete</Badge>
              : currentPhase === 1 ? <Badge variant="processing">{buildProgress?.progress || 0}%</Badge>
              : <Badge variant="secondary">Waiting</Badge>}
          </div>
          <div className="text-xs text-gray-400 font-mono mb-2">POST /api/graph/build</div>
          <p className="text-xs text-gray-500 leading-relaxed mb-4">
            Build knowledge graph from the generated ontology using Zep, extracting entities and relations with temporal memory and community summaries.
          </p>
          <div className="grid grid-cols-3 gap-3 bg-gray-50 p-4 rounded-md">
            {[
              { label: "Entity Nodes", value: graphStats.nodes },
              { label: "Relation Edges", value: graphStats.edges },
              { label: "Schema Types", value: graphStats.types },
            ].map((stat) => (
              <div key={stat.label} className="text-center">
                <span className="block text-xl font-bold font-mono">{stat.value}</span>
                <span className="text-[9px] text-gray-400 uppercase mt-1 block">{stat.label}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Step 03: Complete */}
        <div className={`bg-white rounded-lg p-5 shadow-sm border transition-all ${currentPhase === 2 ? "border-primary shadow-md" : "border-gray-200"}`}>
          <div className="flex justify-between items-center mb-4">
            <div className="flex items-center gap-3">
              <span className={`font-mono text-xl font-bold ${currentPhase >= 2 ? "text-black" : "text-gray-300"}`}>03</span>
              <span className="font-semibold text-sm">Build Complete</span>
            </div>
            {currentPhase >= 2 && <Badge variant="processing">Ready</Badge>}
          </div>
          <div className="text-xs text-gray-400 font-mono mb-2">POST /api/simulation/create</div>
          <p className="text-xs text-gray-500 mb-4">Graph build complete. Proceed to environment setup.</p>
          <Button
            className="w-full rounded"
            disabled={currentPhase < 2 || creatingSimulation}
            onClick={handleEnterEnvSetup}
          >
            {creatingSimulation ? "Creating..." : "Enter Environment Setup"}
          </Button>
        </div>
      </div>

      {/* System Logs */}
      <div className="bg-black text-gray-300 p-4 font-mono border-t border-gray-800 flex-shrink-0">
        <div className="flex justify-between border-b border-gray-800 pb-2 mb-2 text-[10px] text-gray-600">
          <span>SYSTEM DASHBOARD</span>
          <span>{projectData?.project_id || "NO_PROJECT"}</span>
        </div>
        <div ref={logRef} className="flex flex-col gap-1 h-20 overflow-y-auto pr-1 scrollbar-thin">
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
