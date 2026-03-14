"use client"

/* eslint-disable @typescript-eslint/no-explicit-any */
import { useRef, useEffect, useState, useMemo, useCallback } from "react"
import * as d3 from "d3"
import { RefreshCw, Maximize2 } from "lucide-react"

interface GraphPanelProps {
  graphData: any
  loading: boolean
  currentPhase: number
  isSimulating?: boolean
  onRefresh: () => void
  onToggleMaximize: () => void
}

const COLORS = [
  "#3B82F6", "#7C3AED", "#059669", "#DC2626", "#D97706",
  "#2563EB", "#9333EA", "#0D9488", "#E11D48", "#EA580C",
]

export default function GraphPanel({ graphData, loading, currentPhase, isSimulating, onRefresh, onToggleMaximize }: GraphPanelProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const svgRef = useRef<SVGSVGElement>(null)
  const simulationRef = useRef<d3.Simulation<any, any> | null>(null)
  const [selectedItem, setSelectedItem] = useState<any>(null)
  const [showEdgeLabels, setShowEdgeLabels] = useState(true)

  const entityTypes = useMemo(() => {
    if (!graphData?.nodes) return []
    const typeMap: Record<string, { name: string; count: number; color: string }> = {}
    graphData.nodes.forEach((node: any) => {
      const type = node.labels?.find((l: string) => l !== "Entity") || "Entity"
      if (!typeMap[type]) {
        typeMap[type] = { name: type, count: 0, color: COLORS[Object.keys(typeMap).length % COLORS.length] }
      }
      typeMap[type].count++
    })
    return Object.values(typeMap)
  }, [graphData])

  const getColor = useCallback(
    (type: string) => entityTypes.find((t) => t.name === type)?.color || "#999",
    [entityTypes]
  )

  const renderGraph = useCallback(() => {
    if (!svgRef.current || !containerRef.current || !graphData) return
    if (simulationRef.current) simulationRef.current.stop()

    const container = containerRef.current
    const width = container.clientWidth
    const height = container.clientHeight
    const svg = d3.select(svgRef.current).attr("width", width).attr("height", height)
    svg.selectAll("*").remove()

    const nodesData = graphData.nodes || []
    const edgesData = graphData.edges || []
    if (nodesData.length === 0) return

    const nodeMap: Record<string, any> = {}
    nodesData.forEach((n: any) => (nodeMap[n.uuid] = n))

    const nodes = nodesData.map((n: any) => ({
      id: n.uuid,
      name: n.name || "Unnamed",
      type: n.labels?.find((l: string) => l !== "Entity") || "Entity",
      rawData: n,
    }))

    const nodeIds = new Set(nodes.map((n: any) => n.id))
    const edgePairCount: Record<string, number> = {}
    const edgePairIndex: Record<string, number> = {}
    const processedSelfLoopNodes = new Set<string>()
    const selfLoopEdges: Record<string, any[]> = {}

    const tempEdges = edgesData.filter(
      (e: any) => nodeIds.has(e.source_node_uuid) && nodeIds.has(e.target_node_uuid)
    )

    tempEdges.forEach((e: any) => {
      if (e.source_node_uuid === e.target_node_uuid) {
        if (!selfLoopEdges[e.source_node_uuid]) selfLoopEdges[e.source_node_uuid] = []
        selfLoopEdges[e.source_node_uuid].push({
          ...e,
          source_name: nodeMap[e.source_node_uuid]?.name,
          target_name: nodeMap[e.target_node_uuid]?.name,
        })
      } else {
        const pairKey = [e.source_node_uuid, e.target_node_uuid].sort().join("_")
        edgePairCount[pairKey] = (edgePairCount[pairKey] || 0) + 1
      }
    })

    const edges: any[] = []
    tempEdges.forEach((e: any) => {
      const isSelfLoop = e.source_node_uuid === e.target_node_uuid
      if (isSelfLoop) {
        if (processedSelfLoopNodes.has(e.source_node_uuid)) return
        processedSelfLoopNodes.add(e.source_node_uuid)
        const allSelf = selfLoopEdges[e.source_node_uuid]
        edges.push({
          source: e.source_node_uuid,
          target: e.target_node_uuid,
          name: `Self (${allSelf.length})`,
          curvature: 0,
          isSelfLoop: true,
          rawData: { isSelfLoopGroup: true, source_name: nodeMap[e.source_node_uuid]?.name, selfLoopCount: allSelf.length, selfLoopEdges: allSelf },
        })
        return
      }
      const pairKey = [e.source_node_uuid, e.target_node_uuid].sort().join("_")
      const total = edgePairCount[pairKey]
      const idx = edgePairIndex[pairKey] || 0
      edgePairIndex[pairKey] = idx + 1
      let curvature = 0
      if (total > 1) {
        const range = Math.min(1.2, 0.6 + total * 0.15)
        curvature = ((idx / (total - 1)) - 0.5) * range * 2
        if (e.source_node_uuid > e.target_node_uuid) curvature = -curvature
      }
      edges.push({
        source: e.source_node_uuid,
        target: e.target_node_uuid,
        name: e.name || e.fact_type || "RELATED",
        curvature,
        isSelfLoop: false,
        pairTotal: total,
        rawData: { ...e, source_name: nodeMap[e.source_node_uuid]?.name, target_name: nodeMap[e.target_node_uuid]?.name },
      })
    })

    const simulation = d3
      .forceSimulation(nodes)
      .force("link", d3.forceLink(edges).id((d: any) => d.id).distance((d: any) => 150 + ((d.pairTotal || 1) - 1) * 50))
      .force("charge", d3.forceManyBody().strength(-400))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collide", d3.forceCollide(50))
      .force("x", d3.forceX(width / 2).strength(0.04))
      .force("y", d3.forceY(height / 2).strength(0.04))
    simulationRef.current = simulation

    const g = svg.append("g")
    svg.call(
      d3.zoom<SVGSVGElement, unknown>().scaleExtent([0.1, 4]).on("zoom", (event) => {
        g.attr("transform", event.transform)
      }) as any
    )

    const getLinkPath = (d: any) => {
      const sx = d.source.x, sy = d.source.y, tx = d.target.x, ty = d.target.y
      if (d.isSelfLoop) {
        const r = 30
        return `M${sx + 8},${sy - 4} A${r},${r} 0 1,1 ${sx + 8},${sy + 4}`
      }
      if (d.curvature === 0) return `M${sx},${sy} L${tx},${ty}`
      const dx = tx - sx, dy = ty - sy
      const dist = Math.sqrt(dx * dx + dy * dy) || 1
      const ratio = 0.25 + (d.pairTotal || 1) * 0.05
      const base = Math.max(35, dist * ratio)
      const ox = (-dy / dist) * d.curvature * base
      const oy = (dx / dist) * d.curvature * base
      const cx = (sx + tx) / 2 + ox, cy = (sy + ty) / 2 + oy
      return `M${sx},${sy} Q${cx},${cy} ${tx},${ty}`
    }

    const getLinkMid = (d: any) => {
      const sx = d.source.x, sy = d.source.y, tx = d.target.x, ty = d.target.y
      if (d.isSelfLoop) return { x: sx + 70, y: sy }
      if (d.curvature === 0) return { x: (sx + tx) / 2, y: (sy + ty) / 2 }
      const dx = tx - sx, dy = ty - sy
      const dist = Math.sqrt(dx * dx + dy * dy) || 1
      const ratio = 0.25 + (d.pairTotal || 1) * 0.05
      const base = Math.max(35, dist * ratio)
      const ox = (-dy / dist) * d.curvature * base
      const oy = (dx / dist) * d.curvature * base
      const cx = (sx + tx) / 2 + ox, cy = (sy + ty) / 2 + oy
      return { x: 0.25 * sx + 0.5 * cx + 0.25 * tx, y: 0.25 * sy + 0.5 * cy + 0.25 * ty }
    }

    const linkGroup = g.append("g")
    const link = linkGroup.selectAll("path").data(edges).enter().append("path")
      .attr("stroke", "#C0C0C0").attr("stroke-width", 1.5).attr("fill", "none").style("cursor", "pointer")
      .on("click", (event: any, d: any) => {
        event.stopPropagation()
        setSelectedItem({ type: "edge", data: d.rawData })
      })

    const linkLabelBg = linkGroup.selectAll("rect").data(edges).enter().append("rect")
      .attr("fill", "rgba(255,255,255,0.95)").attr("rx", 3).attr("ry", 3)
      .style("display", showEdgeLabels ? "block" : "none")

    const linkLabels = linkGroup.selectAll("text").data(edges).enter().append("text")
      .text((d: any) => d.name).attr("font-size", "9px").attr("fill", "#666")
      .attr("text-anchor", "middle").attr("dominant-baseline", "middle")
      .style("pointer-events", "none").style("font-family", "system-ui, sans-serif")
      .style("display", showEdgeLabels ? "block" : "none")

    const nodeGroup = g.append("g")
    const node = nodeGroup.selectAll("circle").data(nodes).enter().append("circle")
      .attr("r", 10).attr("fill", (d: any) => getColor(d.type)).attr("stroke", "#fff").attr("stroke-width", 2.5)
      .style("cursor", "pointer")
      .call(
        d3.drag<SVGCircleElement, any>()
          .on("start", (event, d) => { d.fx = d.x; d.fy = d.y })
          .on("drag", (event, d) => {
            if (!simulation.alpha()) simulation.alphaTarget(0.3).restart()
            d.fx = event.x; d.fy = event.y
          })
          .on("end", (_, d) => { simulation.alphaTarget(0); d.fx = null; d.fy = null }) as any
      )
      .on("click", (event: any, d: any) => {
        event.stopPropagation()
        node.attr("stroke", "#fff").attr("stroke-width", 2.5)
        d3.select(event.target).attr("stroke", "#E91E63").attr("stroke-width", 4)
        setSelectedItem({ type: "node", data: d.rawData, entityType: d.type, color: getColor(d.type) })
      })

    nodeGroup.selectAll("text").data(nodes).enter().append("text")
      .text((d: any) => d.name.length > 8 ? d.name.substring(0, 8) + "..." : d.name)
      .attr("font-size", "11px").attr("fill", "#333").attr("font-weight", "500")
      .attr("dx", 14).attr("dy", 4).style("pointer-events", "none").style("font-family", "system-ui, sans-serif")

    simulation.on("tick", () => {
      link.attr("d", (d: any) => getLinkPath(d))
      linkLabels.each(function (d: any) {
        const mid = getLinkMid(d)
        d3.select(this).attr("x", mid.x).attr("y", mid.y)
      })
      linkLabelBg.each(function (d: any, i: number) {
        const mid = getLinkMid(d)
        const textEl = linkLabels.nodes()[i] as SVGTextElement
        if (textEl) {
          const bbox = textEl.getBBox()
          d3.select(this).attr("x", mid.x - bbox.width / 2 - 4).attr("y", mid.y - bbox.height / 2 - 2)
            .attr("width", bbox.width + 8).attr("height", bbox.height + 4)
        }
      })
      node.attr("cx", (d: any) => d.x).attr("cy", (d: any) => d.y)
      nodeGroup.selectAll("text").attr("x", (d: any) => d.x).attr("y", (d: any) => d.y)
    })

    svg.on("click", () => {
      setSelectedItem(null)
      node.attr("stroke", "#fff").attr("stroke-width", 2.5)
    })
  }, [graphData, getColor, showEdgeLabels])

  useEffect(() => {
    renderGraph()
    const handleResize = () => renderGraph()
    window.addEventListener("resize", handleResize)
    return () => {
      window.removeEventListener("resize", handleResize)
      if (simulationRef.current) simulationRef.current.stop()
    }
  }, [renderGraph])

  return (
    <div className="relative w-full h-full bg-[#FAFAFA]" style={{ backgroundImage: "radial-gradient(#D0D0D0 1.5px, transparent 1.5px)", backgroundSize: "24px 24px" }}>
      {/* Header */}
      <div className="absolute top-0 left-0 right-0 p-4 z-10 flex justify-between items-center bg-gradient-to-b from-white/95 to-transparent pointer-events-none">
        <span className="text-sm font-semibold text-gray-700 pointer-events-auto">Graph Relationship Visualization</span>
        <div className="pointer-events-auto flex gap-2.5 items-center">
          <button className="h-8 px-3 border border-gray-200 bg-white rounded-md flex items-center gap-1.5 text-gray-500 hover:text-black hover:border-gray-400 text-xs transition-colors" onClick={onRefresh} disabled={loading}>
            <RefreshCw size={13} className={loading ? "animate-spin" : ""} />
            <span>Refresh</span>
          </button>
          <button className="h-8 w-8 border border-gray-200 bg-white rounded-md flex items-center justify-center text-gray-500 hover:text-black hover:border-gray-400 transition-colors" onClick={onToggleMaximize}>
            <Maximize2 size={13} />
          </button>
        </div>
      </div>

      {/* Graph Container */}
      <div ref={containerRef} className="w-full h-full">
        {graphData ? (
          <div className="relative w-full h-full">
            <svg ref={svgRef} className="w-full h-full block" />
            {(currentPhase === 1 || isSimulating) && (
              <div className="absolute bottom-40 left-1/2 -translate-x-1/2 bg-black/65 backdrop-blur-sm text-white px-5 py-2.5 rounded-full text-sm flex items-center gap-2.5 shadow-lg border border-white/10 font-medium z-[100]">
                <span className="animate-[breathe_2s_ease-in-out_infinite] text-green-400">&#9679;</span>
                {isSimulating ? "GraphRAG memory updating in real-time" : "Updating in real-time..."}
              </div>
            )}
          </div>
        ) : loading ? (
          <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-center text-gray-400">
            <div className="w-10 h-10 border-3 border-gray-200 border-t-primary rounded-full animate-spin mx-auto mb-4" />
            <p>Loading graph data...</p>
          </div>
        ) : (
          <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-center text-gray-400">
            <div className="text-5xl mb-4 opacity-20">&#10070;</div>
            <p>Waiting for ontology generation...</p>
          </div>
        )}
      </div>

      {/* Legend */}
      {graphData && entityTypes.length > 0 && (
        <div className="absolute bottom-6 left-6 bg-white/95 p-3 px-4 rounded-lg border border-gray-200 shadow-md z-10">
          <span className="block text-[11px] font-semibold text-primary mb-2.5 uppercase tracking-wider">Entity Types</span>
          <div className="flex flex-wrap gap-x-4 gap-y-2 max-w-xs">
            {entityTypes.map((t) => (
              <div key={t.name} className="flex items-center gap-1.5 text-xs text-gray-600">
                <span className="w-2.5 h-2.5 rounded-full flex-shrink-0" style={{ background: t.color }} />
                <span className="whitespace-nowrap">{t.name}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Edge Labels Toggle */}
      {graphData && (
        <div className="absolute top-16 right-5 flex items-center gap-2.5 bg-white px-3.5 py-2 rounded-full border border-gray-200 shadow-sm z-10">
          <label className="relative inline-block w-10 h-[22px] cursor-pointer">
            <input type="checkbox" checked={showEdgeLabels} onChange={(e) => setShowEdgeLabels(e.target.checked)} className="opacity-0 w-0 h-0 peer" />
            <span className="absolute inset-0 bg-gray-200 rounded-full transition-colors peer-checked:bg-primary before:absolute before:content-[''] before:h-4 before:w-4 before:left-[3px] before:bottom-[3px] before:bg-white before:rounded-full before:transition-transform peer-checked:before:translate-x-[18px]" />
          </label>
          <span className="text-xs text-gray-500">Edge Labels</span>
        </div>
      )}

      {/* Detail Panel */}
      {selectedItem && (
        <div className="absolute top-16 right-5 w-80 max-h-[calc(100%-100px)] bg-white border border-gray-200 rounded-xl shadow-xl overflow-hidden z-20 flex flex-col text-sm">
          <div className="flex justify-between items-center px-4 py-3 bg-gray-50 border-b border-gray-200 flex-shrink-0">
            <span className="font-semibold text-gray-700 text-sm">
              {selectedItem.type === "node" ? "Node Details" : "Relationship"}
            </span>
            {selectedItem.type === "node" && (
              <span className="px-2.5 py-0.5 rounded-full text-[11px] text-white" style={{ background: selectedItem.color }}>
                {selectedItem.entityType}
              </span>
            )}
            <button className="text-gray-400 hover:text-gray-700 text-xl leading-none" onClick={() => setSelectedItem(null)}>
              &times;
            </button>
          </div>
          <div className="p-4 overflow-y-auto flex-1 space-y-3">
            {selectedItem.type === "node" ? (
              <>
                <div><span className="text-xs text-gray-500">Name:</span> <span className="text-gray-800">{selectedItem.data.name}</span></div>
                <div><span className="text-xs text-gray-500">UUID:</span> <span className="text-gray-500 font-mono text-[11px] break-all">{selectedItem.data.uuid}</span></div>
                {selectedItem.data.summary && (
                  <div className="pt-3 border-t border-gray-100">
                    <span className="text-xs text-gray-500 block mb-1">Summary:</span>
                    <p className="text-xs text-gray-600 leading-relaxed">{selectedItem.data.summary}</p>
                  </div>
                )}
                {selectedItem.data.labels?.length > 0 && (
                  <div className="pt-3 border-t border-gray-100">
                    <span className="text-xs text-gray-500 block mb-2">Labels:</span>
                    <div className="flex flex-wrap gap-2">
                      {selectedItem.data.labels.map((l: string) => (
                        <span key={l} className="px-3 py-1 bg-gray-100 border border-gray-200 rounded-full text-[11px] text-gray-600">{l}</span>
                      ))}
                    </div>
                  </div>
                )}
              </>
            ) : (
              <>
                <div className="bg-gray-50 p-3 rounded-lg text-sm text-gray-700 leading-relaxed break-words">
                  {selectedItem.data.source_name} &rarr; {selectedItem.data.name || "RELATED"} &rarr; {selectedItem.data.target_name}
                </div>
                {selectedItem.data.uuid && <div><span className="text-xs text-gray-500">UUID:</span> <span className="text-gray-500 font-mono text-[11px] break-all">{selectedItem.data.uuid}</span></div>}
                {selectedItem.data.fact && <div><span className="text-xs text-gray-500">Fact:</span> <p className="text-xs text-gray-600 leading-relaxed">{selectedItem.data.fact}</p></div>}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
