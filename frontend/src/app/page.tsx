"use client"

import { useState, useRef, useCallback } from "react"
import { useRouter } from "next/navigation"
import { motion } from "motion/react"
import { Upload, ArrowDown, ArrowRight, FileText, X } from "lucide-react"
import { useAppStore } from "@/lib/store"
import { Button } from "@/components/ui/button"
import HistoryPanel from "@/components/HistoryPanel"

const WORKFLOW_STEPS = [
  { num: "01", title: "Graph Build", desc: "Reality seed extraction & individual/group memory injection & GraphRAG construction" },
  { num: "02", title: "Env Setup", desc: "Entity extraction & profile generation & environment config agent parameter injection" },
  { num: "03", title: "Simulation", desc: "Dual-platform parallel simulation & auto-parse prediction needs & dynamic memory update" },
  { num: "04", title: "Report", desc: "ReportAgent with rich toolset interacts deeply with the post-simulation environment" },
  { num: "05", title: "Interaction", desc: "Chat with any agent in the simulated world & converse with the ReportAgent" },
]

export default function HomePage() {
  const router = useRouter()
  const setPendingUpload = useAppStore((s) => s.setPendingUpload)

  const [files, setFiles] = useState<File[]>([])
  const [requirement, setRequirement] = useState("")
  const [isDragOver, setIsDragOver] = useState(false)
  const [loading, setLoading] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  const canSubmit = requirement.trim() !== "" && files.length > 0

  const addFiles = useCallback((newFiles: File[]) => {
    const valid = newFiles.filter((f) => {
      const ext = f.name.split(".").pop()?.toLowerCase()
      return ["pdf", "md", "txt"].includes(ext || "")
    })
    setFiles((prev) => [...prev, ...valid])
  }, [])

  const removeFile = (index: number) => setFiles((prev) => prev.filter((_, i) => i !== index))

  const handleStart = () => {
    if (!canSubmit || loading) return
    setLoading(true)
    setPendingUpload(files, requirement)
    router.push("/process/new")
  }

  return (
    <div className="min-h-screen bg-white">
      {/* Navbar */}
      <nav className="h-[60px] bg-black text-white flex items-center justify-between px-10">
        <span className="font-mono font-extrabold text-lg tracking-wider">MIROFISH</span>
        <a
          href="https://github.com/666ghj/MiroFish"
          target="_blank"
          rel="noreferrer"
          className="font-mono text-sm hover:opacity-80 transition-opacity flex items-center gap-2"
        >
          Visit our GitHub
          <span className="text-xs">&#8599;</span>
        </a>
      </nav>

      <div className="max-w-[1400px] mx-auto px-10 py-16">
        {/* Hero Section */}
        <motion.section
          className="flex flex-col lg:flex-row justify-between mb-20"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <div className="flex-1 lg:pr-16">
            <div className="flex items-center gap-4 mb-6 font-mono text-xs">
              <span className="bg-primary text-white px-2.5 py-1 font-bold tracking-wider text-[11px]">
                Swarm Intelligence Engine
              </span>
              <span className="text-gray-400">/ v0.1-preview</span>
            </div>

            <h1 className="text-5xl lg:text-6xl font-medium leading-tight mb-10 tracking-tight">
              Upload Any Report
              <br />
              <span className="bg-gradient-to-r from-black to-gray-500 bg-clip-text text-transparent">
                Simulate the Future
              </span>
            </h1>

            <div className="text-base text-gray-500 max-w-xl mb-12 leading-relaxed space-y-4">
              <p>
                Even with just a paragraph of text, <strong className="text-black">MiroFish</strong> can generate a parallel
                world of up to <span className="text-primary font-bold font-mono">1M agents</span> from the reality seeds
                within. Inject variables from a god&apos;s-eye view to find{" "}
                <code className="bg-gray-100 px-1.5 py-0.5 rounded text-xs text-black font-semibold">
                  &quot;local optima&quot;
                </code>{" "}
                in complex group interactions.
              </p>
              <p className="text-lg font-medium text-black border-l-3 border-primary pl-4">
                Let the future rehearse among agents, let decisions win after a hundred battles
                <span className="text-primary animate-[blink_1s_step-end_infinite] font-bold">_</span>
              </p>
            </div>

            <div className="w-4 h-4 bg-primary" />
          </div>

          <div className="flex-shrink-0 lg:w-[40%] flex flex-col justify-between items-end">
            <div className="w-full flex justify-end pr-10 mb-8">
              <div className="w-full max-w-[400px] aspect-square bg-gradient-to-br from-blue-50 to-indigo-100 rounded-2xl flex items-center justify-center">
                <span className="text-6xl font-mono font-black text-primary/20">MF</span>
              </div>
            </div>
            <button
              className="w-10 h-10 border border-gray-200 flex items-center justify-center text-primary hover:border-primary transition-colors"
              onClick={() => window.scrollTo({ top: document.body.scrollHeight, behavior: "smooth" })}
            >
              <ArrowDown size={16} />
            </button>
          </div>
        </motion.section>

        {/* Dashboard Section */}
        <motion.section
          className="flex flex-col lg:flex-row gap-16 border-t border-gray-200 pt-16"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.6, delay: 0.2 }}
        >
          {/* Left Panel - Status & Steps */}
          <div className="lg:w-[40%]">
            <div className="flex items-center gap-2 font-mono text-xs text-gray-400 mb-5">
              <span className="text-primary text-xs">&#9632;</span> System Status
            </div>

            <h2 className="text-2xl font-medium mb-3">Ready</h2>
            <p className="text-gray-500 text-sm mb-6 leading-relaxed">
              Prediction engine standing by. Upload unstructured data to initialize the simulation sequence.
            </p>

            <div className="flex gap-5 mb-6">
              <div className="border border-gray-200 px-7 py-5">
                <div className="text-2xl font-medium font-mono mb-1">Low Cost</div>
                <div className="text-xs text-gray-400">Avg $5/simulation</div>
              </div>
              <div className="border border-gray-200 px-7 py-5">
                <div className="text-2xl font-medium font-mono mb-1">Scalable</div>
                <div className="text-xs text-gray-400">Up to 1M agents</div>
              </div>
            </div>

            <div className="border border-gray-200 p-7">
              <div className="flex items-center gap-2 font-mono text-xs text-gray-400 mb-6">
                <span>&#9671;</span> Workflow Sequence
              </div>
              <div className="space-y-5">
                {WORKFLOW_STEPS.map((step) => (
                  <div key={step.num} className="flex items-start gap-5">
                    <span className="font-mono font-bold text-black/30">{step.num}</span>
                    <div>
                      <div className="font-medium text-sm mb-1">{step.title}</div>
                      <div className="text-xs text-gray-500">{step.desc}</div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Right Panel - Console */}
          <div className="lg:w-[60%]">
            <div className="border border-gray-300 p-2">
              {/* Upload Zone */}
              <div className="p-5">
                <div className="flex justify-between mb-4 font-mono text-[11px] text-gray-500">
                  <span>01 / Reality Seeds</span>
                  <span>Formats: PDF, MD, TXT</span>
                </div>

                <div
                  className={`border border-dashed min-h-[200px] flex items-center justify-center cursor-pointer transition-all ${
                    isDragOver ? "border-primary bg-blue-50" : files.length > 0 ? "border-gray-300 bg-gray-50 items-start" : "border-gray-300 bg-gray-50 hover:bg-gray-100 hover:border-gray-400"
                  }`}
                  onDragOver={(e) => { e.preventDefault(); if (!loading) setIsDragOver(true) }}
                  onDragLeave={(e) => { e.preventDefault(); setIsDragOver(false) }}
                  onDrop={(e) => {
                    e.preventDefault()
                    setIsDragOver(false)
                    if (!loading) addFiles(Array.from(e.dataTransfer.files))
                  }}
                  onClick={() => !loading && fileInputRef.current?.click()}
                >
                  <input
                    ref={fileInputRef}
                    type="file"
                    multiple
                    accept=".pdf,.md,.txt"
                    className="hidden"
                    disabled={loading}
                    onChange={(e) => {
                      if (e.target.files) addFiles(Array.from(e.target.files))
                      e.target.value = ""
                    }}
                  />

                  {files.length === 0 ? (
                    <div className="text-center">
                      <div className="w-10 h-10 border border-gray-200 flex items-center justify-center mx-auto mb-4 text-gray-400">
                        <Upload size={16} />
                      </div>
                      <div className="text-sm font-medium mb-1">Drag & drop files</div>
                      <div className="text-xs text-gray-400 font-mono">or click to browse</div>
                    </div>
                  ) : (
                    <div className="w-full p-4 space-y-2">
                      {files.map((file, i) => (
                        <div key={i} className="flex items-center bg-white px-3 py-2 border border-gray-100 font-mono text-sm">
                          <FileText size={14} className="text-gray-400 mr-2" />
                          <span className="flex-1 truncate">{file.name}</span>
                          <button
                            onClick={(e) => { e.stopPropagation(); removeFile(i) }}
                            className="text-gray-400 hover:text-gray-600"
                          >
                            <X size={14} />
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              {/* Divider */}
              <div className="flex items-center my-2">
                <div className="flex-1 h-px bg-gray-100" />
                <span className="px-4 text-[10px] font-mono text-gray-300 tracking-wider">Parameters</span>
                <div className="flex-1 h-px bg-gray-100" />
              </div>

              {/* Requirement Input */}
              <div className="p-5">
                <div className="font-mono text-[11px] text-gray-500 mb-4">
                  &gt;_ 02 / Simulation Prompt
                </div>
                <div className="relative border border-gray-200 bg-gray-50">
                  <textarea
                    value={requirement}
                    onChange={(e) => setRequirement(e.target.value)}
                    className="w-full border-none bg-transparent p-5 font-mono text-sm leading-relaxed resize-y outline-none min-h-[150px]"
                    placeholder="// Describe your simulation or prediction needs in natural language..."
                    disabled={loading}
                  />
                  <div className="absolute bottom-2.5 right-4 font-mono text-[10px] text-gray-300">
                    Engine: MiroFish-V1.0
                  </div>
                </div>
              </div>

              {/* Start Button */}
              <div className="px-5 pb-5">
                <Button
                  className="w-full h-14 text-base font-mono font-bold tracking-wider flex justify-between items-center rounded-none"
                  disabled={!canSubmit || loading}
                  onClick={handleStart}
                >
                  <span>{loading ? "Initializing..." : "Start Engine"}</span>
                  <ArrowRight size={18} />
                </Button>
              </div>
            </div>
          </div>
        </motion.section>

        {/* History Section */}
        <HistoryPanel />
      </div>
    </div>
  )
}
