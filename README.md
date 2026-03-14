# SwarmMirror

A next-generation **Swarm Intelligence Engine** that predicts anything by simulating high-fidelity digital worlds powered by multi-agent AI.

Upload seed materials (research reports, news articles, stories), describe what you want to predict, and SwarmMirror will construct a parallel digital world where thousands of autonomous agents with independent personalities, long-term memory, and behavioral logic interact and evolve on simulated social media platforms. Observe emergent group dynamics from a "god's eye view" and generate detailed predictive analysis reports.

## Features

- **Knowledge Graph Construction** — Automatically extract entities and relationships from uploaded documents using LLM-powered ontology generation and Zep Cloud graph storage
- **Autonomous Agent Profiles** — Generate detailed agent personas (personality, MBTI, profession, interests) from graph entities via LLM
- **Dual-Platform Simulation** — Run parallel Twitter and Reddit simulations where agents autonomously post, comment, like, follow, and interact
- **Intelligent Report Generation** — ReACT-pattern Report Agent with graph search tools produces comprehensive markdown analysis reports
- **Interactive Exploration** — Chat with the Report Agent or interview individual simulation agents to explore scenarios and get deeper insights
- **Real-time Monitoring** — Track simulation progress, view agent action timelines, and inspect round-by-round activity
- **D3.js Graph Visualization** — Interactive force-directed graph with zoom, pan, node filtering, and relationship exploration

## Tech Stack

| Layer | Technology |
|-------|-----------|
| **Backend** | Rust (Axum, Tokio, reqwest, serde) |
| **Frontend** | Next.js 15, TypeScript, Tailwind CSS v4 |
| **UI Components** | shadcn/ui (Radix + Tailwind) |
| **Animations** | Motion (framer-motion) |
| **Graph Visualization** | D3.js |
| **State Management** | Zustand |
| **Package Manager** | Bun |
| **LLM Integration** | OpenAI-compatible API (Aliyun DashScope, OpenAI, etc.) |
| **Knowledge Graph** | Zep Cloud |
| **Deployment** | Cloudflare Workers ready |

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Frontend (Next.js)                 │
│  Home → Graph Build → Env Setup → Simulation →      │
│  Report → Interaction                                │
│  [shadcn/ui] [D3.js] [Motion] [Zustand]             │
└──────────────────────┬──────────────────────────────┘
                       │ REST API (port 3000 → 5001)
┌──────────────────────┴──────────────────────────────┐
│                  Backend (Rust/Axum)                  │
│                                                      │
│  ┌─────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │Graph API│  │Simulation API│  │  Report API    │  │
│  │10 routes│  │  34 routes   │  │  18 routes     │  │
│  └────┬────┘  └──────┬───────┘  └───────┬────────┘  │
│       │              │                  │            │
│  ┌────┴──────────────┴──────────────────┴────────┐  │
│  │              Service Layer                     │  │
│  │  LLM Client · Ontology Generator              │  │
│  │  Graph Builder · Entity Reader                 │  │
│  │  Profile Generator · Config Generator          │  │
│  │  Simulation Runner · Report Agent              │  │
│  │  Zep Tools · Text Processor                    │  │
│  └───────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
         │                              │
    ┌────┴────┐                   ┌─────┴─────┐
    │ Zep Cloud│                  │  LLM API  │
    │ (Graph)  │                  │ (OpenAI)  │
    └─────────┘                   └───────────┘
```

## Workflow

1. **Upload & Analyze** — Upload documents (PDF/MD/TXT), describe your prediction goal. LLM generates an ontology with 10 entity types and 6-10 relationship types.

2. **Build Knowledge Graph** — Text is chunked and fed to Zep Cloud to construct a knowledge graph with entities and relationships.

3. **Generate Agent Profiles** — Entities are extracted from the graph, and LLM generates detailed social media personas for each (personality, demographics, behavioral traits).

4. **Run Simulation** — Agents interact on simulated Twitter/Reddit platforms across multiple rounds. Each agent autonomously decides actions (post, comment, like, follow, etc.) based on their persona.

5. **Generate Report** — A ReACT-pattern Report Agent searches the knowledge graph, analyzes simulation results, and produces a comprehensive markdown report.

6. **Explore & Interact** — Chat with the Report Agent for deeper analysis, or interview individual simulation agents to understand their perspectives.

## Quick Start

### Prerequisites

| Tool | Version | Check |
|------|---------|-------|
| **Rust** | Latest stable | `rustc --version` |
| **Bun** | 1.0+ | `bun --version` |
| **Node.js** | 18+ | `node -v` |

### 1. Configure Environment

```bash
cp .env.example .env
# Edit .env with your API keys
```

**Required environment variables:**

```env
# LLM API (any OpenAI-compatible endpoint)
LLM_API_KEY=your_api_key
LLM_BASE_URL=https://dashscope.aliyuncs.com/compatible-mode/v1
LLM_MODEL_NAME=qwen-plus

# Zep Cloud (free tier available at https://app.getzep.com/)
ZEP_API_KEY=your_zep_api_key
```

### 2. Install Dependencies

```bash
# Install frontend dependencies
cd frontend && bun install

# Build backend (downloads Rust dependencies automatically)
cd ../backend && cargo build
```

### 3. Start Development Server

```bash
# From project root - starts both frontend and backend
npm run dev
```

| Service | URL |
|---------|-----|
| Frontend | http://localhost:3000 |
| Backend API | http://localhost:5001 |
| Health Check | http://localhost:5001/health |

## API Reference

### Graph API (`/api/graph`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/ontology/generate` | Upload files & generate ontology |
| POST | `/build` | Build knowledge graph |
| GET | `/task/{id}` | Query async task status |
| GET | `/data/{graph_id}` | Get graph nodes & edges |
| GET | `/project/{id}` | Get project details |
| GET | `/project/list` | List all projects |
| DELETE | `/project/{id}` | Delete project |
| DELETE | `/delete/{graph_id}` | Delete graph |

### Simulation API (`/api/simulation`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/create` | Create simulation instance |
| POST | `/prepare` | Prepare environment (async) |
| POST | `/run` | Start simulation (async) |
| POST | `/stop` | Stop running simulation |
| POST | `/interview` | Interview a single agent |
| POST | `/interview/batch` | Interview multiple agents |
| GET | `/entities/{graph_id}` | Get filtered entities |
| GET | `/{id}/profiles` | Get agent profiles |
| GET | `/{id}/config` | Get simulation config |
| GET | `/{id}/actions` | Get simulation actions |
| GET | `/{id}/timeline` | Get event timeline |
| GET | `/{id}/agent-stats` | Get agent statistics |
| GET | `/{id}/posts` | Get social media posts |

### Report API (`/api/report`)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/generate` | Generate analysis report (async) |
| POST | `/chat` | Chat with Report Agent |
| GET | `/{id}` | Get report details |
| GET | `/{id}/sections` | Get report sections |
| GET | `/{id}/progress` | Get generation progress |
| GET | `/{id}/agent-log` | Get agent execution log |
| GET | `/check/{sim_id}` | Check report status |
| GET | `/list` | List all reports |

## Project Structure

```
├── backend/                    # Rust API server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Entry point, routing, CORS
│       ├── config.rs           # Environment configuration
│       ├── error.rs            # Error types
│       ├── api/                # Route handlers
│       │   ├── graph.rs        # Graph & project endpoints
│       │   ├── simulation.rs   # Simulation & interview endpoints
│       │   └── report.rs       # Report & chat endpoints
│       ├── models/             # Data models
│       │   ├── project.rs      # Project persistence
│       │   ├── task.rs         # Async task management
│       │   ├── simulation.rs   # Simulation state machine
│       │   └── report.rs       # Report management
│       └── services/           # Business logic
│           ├── llm_client.rs   # OpenAI-compatible client
│           ├── zep_client.rs   # Zep Cloud API client
│           ├── ontology_generator.rs
│           ├── graph_builder.rs
│           ├── zep_entity_reader.rs
│           ├── profile_generator.rs
│           ├── simulation_runner.rs
│           ├── report_agent.rs
│           └── zep_tools.rs
├── frontend/                   # Next.js application
│   └── src/
│       ├── app/                # Pages (App Router)
│       ├── components/         # UI & feature components
│       │   ├── ui/             # shadcn/ui primitives
│       │   ├── steps/          # Workflow step components
│       │   ├── GraphPanel.tsx  # D3 graph visualization
│       │   └── HistoryPanel.tsx
│       └── lib/                # API clients, store, utilities
├── .env.example                # Environment template
└── package.json                # Root scripts
```

## License

MIT
