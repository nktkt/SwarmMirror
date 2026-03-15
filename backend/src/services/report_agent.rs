use crate::models::report::{Report, ReportManager, ReportStatus};
use crate::services::llm_client::{ChatMessage, LLMClient};
use crate::services::zep_client::ZepClient;
use crate::services::zep_tools::ZepToolsService;
use chrono::Local;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{debug, error, info, warn};

// ═══════════════════════════════════════════════════════════════
// Data Structures
// ═══════════════════════════════════════════════════════════════

/// Report section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSection {
    pub title: String,
    pub content: String,
}

impl ReportSection {
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            content: String::new(),
        }
    }

    pub fn to_dict(&self) -> Value {
        json!({
            "title": self.title,
            "content": self.content
        })
    }

    pub fn to_markdown(&self, level: usize) -> String {
        let hashes = "#".repeat(level);
        let mut md = format!("{} {}\n\n", hashes, self.title);
        if !self.content.is_empty() {
            md += &format!("{}\n\n", self.content);
        }
        md
    }
}

/// Report outline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportOutline {
    pub title: String,
    pub summary: String,
    pub sections: Vec<ReportSection>,
}

impl ReportOutline {
    pub fn to_dict(&self) -> Value {
        json!({
            "title": self.title,
            "summary": self.summary,
            "sections": self.sections.iter().map(|s| s.to_dict()).collect::<Vec<_>>()
        })
    }

    pub fn to_markdown(&self) -> String {
        let mut md = format!("# {}\n\n", self.title);
        md += &format!("> {}\n\n", self.summary);
        for section in &self.sections {
            md += &section.to_markdown(2);
        }
        md
    }
}

// ═══════════════════════════════════════════════════════════════
// ReportLogger - Structured JSON logs (agent_log.jsonl)
// ═══════════════════════════════════════════════════════════════

/// Writes structured JSON logs per action into agent_log.jsonl.
pub struct ReportLogger {
    report_id: String,
    log_file_path: PathBuf,
    start_time: chrono::DateTime<Local>,
}

impl ReportLogger {
    pub async fn new(data_dir: &Path, report_id: &str) -> Self {
        let log_dir = data_dir.join("reports").join(report_id);
        fs::create_dir_all(&log_dir).await.ok();
        Self {
            report_id: report_id.to_string(),
            log_file_path: log_dir.join("agent_log.jsonl"),
            start_time: Local::now(),
        }
    }

    fn elapsed_seconds(&self) -> f64 {
        let elapsed = Local::now().signed_duration_since(self.start_time);
        elapsed.num_milliseconds() as f64 / 1000.0
    }

    async fn log(
        &self,
        action: &str,
        stage: &str,
        details: Value,
        section_title: Option<&str>,
        section_index: Option<usize>,
    ) {
        let entry = json!({
            "timestamp": Local::now().to_rfc3339(),
            "elapsed_seconds": (self.elapsed_seconds() * 100.0).round() / 100.0,
            "report_id": self.report_id,
            "action": action,
            "stage": stage,
            "section_title": section_title,
            "section_index": section_index,
            "details": details
        });
        let line = serde_json::to_string(&entry).unwrap_or_default() + "\n";
        use tokio::io::AsyncWriteExt;
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
            .await
        {
            file.write_all(line.as_bytes()).await.ok();
        }
    }

    pub async fn log_start(
        &self,
        simulation_id: &str,
        graph_id: &str,
        simulation_requirement: &str,
    ) {
        self.log(
            "report_start",
            "pending",
            json!({
                "simulation_id": simulation_id,
                "graph_id": graph_id,
                "simulation_requirement": simulation_requirement,
                "message": "报告生成任务开始"
            }),
            None,
            None,
        )
        .await;
    }

    pub async fn log_planning_start(&self) {
        self.log(
            "planning_start",
            "planning",
            json!({"message": "开始规划报告大纲"}),
            None,
            None,
        )
        .await;
    }

    pub async fn log_planning_context(&self, context: &Value) {
        self.log(
            "planning_context",
            "planning",
            json!({
                "message": "获取模拟上下文信息",
                "context": context
            }),
            None,
            None,
        )
        .await;
    }

    pub async fn log_planning_complete(&self, outline_dict: &Value) {
        self.log(
            "planning_complete",
            "planning",
            json!({
                "message": "大纲规划完成",
                "outline": outline_dict
            }),
            None,
            None,
        )
        .await;
    }

    pub async fn log_section_start(&self, section_title: &str, section_index: usize) {
        self.log(
            "section_start",
            "generating",
            json!({"message": format!("开始生成章节: {}", section_title)}),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_react_thought(
        &self,
        section_title: &str,
        section_index: usize,
        iteration: usize,
        thought: &str,
    ) {
        self.log(
            "react_thought",
            "generating",
            json!({
                "iteration": iteration,
                "thought": thought,
                "message": format!("ReACT 第{}轮思考", iteration)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_tool_call(
        &self,
        section_title: &str,
        section_index: usize,
        tool_name: &str,
        parameters: &Value,
        iteration: usize,
    ) {
        self.log(
            "tool_call",
            "generating",
            json!({
                "iteration": iteration,
                "tool_name": tool_name,
                "parameters": parameters,
                "message": format!("调用工具: {}", tool_name)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_tool_result(
        &self,
        section_title: &str,
        section_index: usize,
        tool_name: &str,
        result: &str,
        iteration: usize,
    ) {
        self.log(
            "tool_result",
            "generating",
            json!({
                "iteration": iteration,
                "tool_name": tool_name,
                "result": result,
                "result_length": result.len(),
                "message": format!("工具 {} 返回结果", tool_name)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_llm_response(
        &self,
        section_title: &str,
        section_index: usize,
        response: &str,
        iteration: usize,
        has_tool_calls: bool,
        has_final_answer: bool,
    ) {
        self.log(
            "llm_response",
            "generating",
            json!({
                "iteration": iteration,
                "response": response,
                "response_length": response.len(),
                "has_tool_calls": has_tool_calls,
                "has_final_answer": has_final_answer,
                "message": format!("LLM 响应 (工具调用: {}, 最终答案: {})", has_tool_calls, has_final_answer)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_section_content(
        &self,
        section_title: &str,
        section_index: usize,
        content: &str,
        tool_calls_count: usize,
    ) {
        self.log(
            "section_content",
            "generating",
            json!({
                "content": content,
                "content_length": content.len(),
                "tool_calls_count": tool_calls_count,
                "message": format!("章节 {} 内容生成完成", section_title)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_section_full_complete(
        &self,
        section_title: &str,
        section_index: usize,
        full_content: &str,
    ) {
        self.log(
            "section_complete",
            "generating",
            json!({
                "content": full_content,
                "content_length": full_content.len(),
                "message": format!("章节 {} 生成完成", section_title)
            }),
            Some(section_title),
            Some(section_index),
        )
        .await;
    }

    pub async fn log_report_complete(&self, total_sections: usize, total_time_seconds: f64) {
        self.log(
            "report_complete",
            "completed",
            json!({
                "total_sections": total_sections,
                "total_time_seconds": (total_time_seconds * 100.0).round() / 100.0,
                "message": "报告生成完成"
            }),
            None,
            None,
        )
        .await;
    }

    pub async fn log_error(&self, error_message: &str, stage: &str, section_title: Option<&str>) {
        self.log(
            "error",
            stage,
            json!({
                "error": error_message,
                "message": format!("发生错误: {}", error_message)
            }),
            section_title,
            None,
        )
        .await;
    }
}

// ═══════════════════════════════════════════════════════════════
// ReportConsoleLogger - Text console logs (console_log.txt)
// ═══════════════════════════════════════════════════════════════

/// Writes text console-style logs into console_log.txt.
pub struct ReportConsoleLogger {
    log_file_path: PathBuf,
}

impl ReportConsoleLogger {
    pub async fn new(data_dir: &Path, report_id: &str) -> Self {
        let log_dir = data_dir.join("reports").join(report_id);
        fs::create_dir_all(&log_dir).await.ok();
        Self {
            log_file_path: log_dir.join("console_log.txt"),
        }
    }

    pub async fn write(&self, level: &str, message: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        let line = format!("[{}] {}: {}\n", timestamp, level, message);
        use tokio::io::AsyncWriteExt;
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file_path)
            .await
        {
            file.write_all(line.as_bytes()).await.ok();
        }
    }

    pub async fn info(&self, message: &str) {
        self.write("INFO", message).await;
    }

    pub async fn warning(&self, message: &str) {
        self.write("WARNING", message).await;
    }

    pub async fn error(&self, message: &str) {
        self.write("ERROR", message).await;
    }
}

// ═══════════════════════════════════════════════════════════════
// Prompt Templates
// ═══════════════════════════════════════════════════════════════

const TOOL_DESC_INSIGHT_FORGE: &str = "\
【深度洞察检索 - 强大的检索工具】
这是我们强大的检索函数，专为深度分析设计。它会：
1. 自动将你的问题分解为多个子问题
2. 从多个维度检索模拟图谱中的信息
3. 整合语义搜索、实体分析、关系链追踪的结果
4. 返回最全面、最深度的检索内容

【使用场景】
- 需要深入分析某个话题
- 需要了解事件的多个方面
- 需要获取支撑报告章节的丰富素材

【返回内容】
- 相关事实原文（可直接引用）
- 核心实体洞察
- 关系链分析";

const TOOL_DESC_PANORAMA_SEARCH: &str = "\
【广度搜索 - 获取全貌视图】
这个工具用于获取模拟结果的完整全貌，特别适合了解事件演变过程。它会：
1. 获取所有相关节点和关系
2. 区分当前有效的事实和历史/过期的事实
3. 帮助你了解舆情是如何演变的

【使用场景】
- 需要了解事件的完整发展脉络
- 需要对比不同阶段的舆情变化
- 需要获取全面的实体和关系信息

【返回内容】
- 当前有效事实（模拟最新结果）
- 历史/过期事实（演变记录）
- 所有涉及的实体";

const TOOL_DESC_QUICK_SEARCH: &str = "\
【简单搜索 - 快速检索】
轻量级的快速检索工具，适合简单、直接的信息查询。

【使用场景】
- 需要快速查找某个具体信息
- 需要验证某个事实
- 简单的信息检索

【返回内容】
- 与查询最相关的事实列表";

const PLAN_SYSTEM_PROMPT: &str = "\
你是一个「未来预测报告」的撰写专家，拥有对模拟世界的「上帝视角」——你可以洞察模拟中每一位Agent的行为、言论和互动。

【核心理念】
我们构建了一个模拟世界，并向其中注入了特定的「模拟需求」作为变量。模拟世界的演化结果，就是对未来可能发生情况的预测。你正在观察的不是\"实验数据\"，而是\"未来的预演\"。

【你的任务】
撰写一份「未来预测报告」，回答：
1. 在我们设定的条件下，未来发生了什么？
2. 各类Agent（人群）是如何反应和行动？
3. 这个模拟揭示了哪些值得关注的未来趋势和风险？

【报告定位】
- 这是一份基于模拟的未来预测报告，揭示\"如果这样，未来会怎样\"
- 聚焦于预测结果：事件走向、群体反应、涌现现象、潜在风险
- 模拟世界中的Agent言行就是对未来人群行为的预测
- 不是对现实世界现状的分析
- 不是泛泛而谈的舆情综述

【章节数量限制】
- 最少2个章节，最多5个章节
- 不需要子章节，每个章节直接撰写完整内容
- 内容要精炼，聚焦于核心预测发现
- 章节结构由你根据预测结果自主设计

请输出JSON格式的报告大纲，格式如下：
{
    \"title\": \"报告标题\",
    \"summary\": \"报告摘要（一句话概括核心预测发现）\",
    \"sections\": [
        {
            \"title\": \"章节标题\",
            \"description\": \"章节内容描述\"
        }
    ]
}

注意：sections数组最少2个，最多5个元素！";

fn build_plan_user_prompt(
    simulation_requirement: &str,
    total_nodes: usize,
    total_edges: usize,
    entity_types: &str,
    total_entities: usize,
    related_facts_json: &str,
) -> String {
    format!(
        "【预测场景设定】\n\
        我们向模拟世界注入的变量（模拟需求）：{}\n\n\
        【模拟世界规模】\n\
        - 参与模拟的实体数量: {}\n\
        - 实体间产生的关系数量: {}\n\
        - 实体类型分布: {}\n\
        - 活跃Agent数量: {}\n\n\
        【模拟预测到的部分未来事实样本】\n\
        {}\n\n\
        请以「上帝视角」审视这个未来预演：\n\
        1. 在我们设定的条件下，未来呈现出了什么样的状态？\n\
        2. 各类人群（Agent）是如何反应和行动的？\n\
        3. 这个模拟揭示了哪些值得关注的未来趋势？\n\n\
        根据预测结果，设计最合适的报告章节结构。\n\n\
        【再次提醒】报告章节数量：最少2个，最多5个，内容要精炼聚焦于核心预测发现。",
        simulation_requirement,
        total_nodes,
        total_edges,
        entity_types,
        total_entities,
        related_facts_json
    )
}

fn build_section_system_prompt(
    report_title: &str,
    report_summary: &str,
    simulation_requirement: &str,
    section_title: &str,
    tools_description: &str,
) -> String {
    format!(
        "你是一个「未来预测报告」的撰写专家，正在撰写报告的一个章节。\n\n\
        报告标题: {}\n\
        报告摘要: {}\n\
        预测场景（模拟需求）: {}\n\
        当前要撰写的章节: {}\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【核心理念】\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        模拟世界是对未来的预演。我们向模拟世界注入了特定条件（模拟需求），\n\
        模拟中Agent的行为和互动，就是对未来人群行为的预测。\n\n\
        你的任务是：\n\
        - 揭示在设定条件下，未来发生了什么\n\
        - 预测各类人群（Agent）是如何反应和行动的\n\
        - 发现值得关注的未来趋势、风险和机会\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【最重要的规则 - 必须遵守】\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        1. 【必须调用工具观察模拟世界】\n\
           - 你正在以「上帝视角」观察未来的预演\n\
           - 所有内容必须来自模拟世界中发生的事件和Agent言行\n\
           - 禁止使用你自己的知识来编写报告内容\n\
           - 每个章节至少调用3次工具（最多5次）来观察模拟的世界\n\n\
        2. 【必须引用Agent的原始言行】\n\
           - Agent的发言和行为是对未来人群行为的预测\n\
           - 在报告中使用引用格式展示这些预测\n\n\
        3. 【语言一致性 - 引用内容必须翻译为报告语言】\n\
           - 工具返回的内容可能包含英文或中英文混杂的表述\n\
           - 必须将其翻译为流畅的中文后再写入报告\n\n\
        4. 【忠实呈现预测结果】\n\
           - 报告内容必须反映模拟世界中的代表未来的模拟结果\n\
           - 不要添加模拟中不存在的信息\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【格式规范】\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        - 禁止在章节内使用任何 Markdown 标题（#、##、###、#### 等）\n\
        - 章节标题由系统自动添加，你只需撰写纯正文内容\n\
        - 使用**粗体**、段落分隔、引用、列表来组织内容\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【可用检索工具】（每章节调用3-5次）\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        {}\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【工作流程】\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        每次回复你只能做以下两件事之一（不可同时做）：\n\n\
        选项A - 调用工具：\n\
        输出你的思考，然后用以下格式调用一个工具：\n\
        <tool_call>\n\
        {{\"name\": \"工具名称\", \"parameters\": {{\"参数名\": \"参数值\"}}}}\n\
        </tool_call>\n\n\
        选项B - 输出最终内容：\n\
        当你已通过工具获取了足够信息，以 \"Final Answer:\" 开头输出章节内容。\n\n\
        严格禁止：\n\
        - 禁止在一次回复中同时包含工具调用和 Final Answer\n\
        - 禁止自己编造工具返回结果\n\
        - 每次回复最多调用一个工具",
        report_title,
        report_summary,
        simulation_requirement,
        section_title,
        tools_description
    )
}

fn build_section_user_prompt(previous_content: &str, section_title: &str) -> String {
    format!(
        "已完成的章节内容（请仔细阅读，避免重复）：\n\
        {}\n\n\
        ═══════════════════════════════════════════════════════════════\n\
        【当前任务】撰写章节: {}\n\
        ═══════════════════════════════════════════════════════════════\n\n\
        【重要提醒】\n\
        1. 仔细阅读上方已完成的章节，避免重复相同的内容！\n\
        2. 开始前必须先调用工具获取模拟数据\n\
        3. 请混合使用不同工具，不要只用一种\n\
        4. 报告内容必须来自检索结果，不要使用自己的知识\n\n\
        【格式警告 - 必须遵守】\n\
        - 不要写任何标题（#、##、###、####都不行）\n\
        - 不要写\"{}\"作为开头\n\
        - 章节标题由系统自动添加\n\
        - 直接写正文，用**粗体**代替小节标题\n\n\
        请开始：\n\
        1. 首先思考（Thought）这个章节需要什么信息\n\
        2. 然后调用工具（Action）获取模拟数据\n\
        3. 收集足够信息后输出 Final Answer（纯正文，无任何标题）",
        previous_content,
        section_title,
        section_title
    )
}

fn build_chat_system_prompt(
    simulation_requirement: &str,
    report_content: &str,
    tools_description: &str,
) -> String {
    format!(
        "你是一个简洁高效的模拟预测助手。\n\n\
        【背景】\n\
        预测条件: {}\n\n\
        【已生成的分析报告】\n\
        {}\n\n\
        【规则】\n\
        1. 优先基于上述报告内容回答问题\n\
        2. 直接回答问题，避免冗长的思考论述\n\
        3. 仅在报告内容不足以回答时，才调用工具检索更多数据\n\
        4. 回答要简洁、清晰、有条理\n\n\
        【可用工具】（仅在需要时使用，最多调用1-2次）\n\
        {}\n\n\
        【工具调用格式】\n\
        <tool_call>\n\
        {{\"name\": \"工具名称\", \"parameters\": {{\"参数名\": \"参数值\"}}}}\n\
        </tool_call>\n\n\
        【回答风格】\n\
        - 简洁直接，不要长篇大论\n\
        - 使用 > 格式引用关键内容\n\
        - 优先给出结论，再解释原因",
        simulation_requirement, report_content, tools_description
    )
}

// ═══════════════════════════════════════════════════════════════
// ReportAgent
// ═══════════════════════════════════════════════════════════════

/// Valid tool names for tool call parsing.
const VALID_TOOL_NAMES: &[&str] = &["insight_forge", "panorama_search", "quick_search"];

/// Tool definition for display.
struct ToolDef {
    name: &'static str,
    description: &'static str,
    params: &'static [(&'static str, &'static str)],
}

const TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "insight_forge",
        description: TOOL_DESC_INSIGHT_FORGE,
        params: &[
            ("query", "你想深入分析的问题或话题"),
            ("report_context", "当前报告章节的上下文（可选）"),
        ],
    },
    ToolDef {
        name: "panorama_search",
        description: TOOL_DESC_PANORAMA_SEARCH,
        params: &[
            ("query", "搜索查询，用于相关性排序"),
            ("include_expired", "是否包含过期/历史内容（默认True）"),
        ],
    },
    ToolDef {
        name: "quick_search",
        description: TOOL_DESC_QUICK_SEARCH,
        params: &[
            ("query", "搜索查询字符串"),
            ("limit", "返回结果数量（可选，默认10）"),
        ],
    },
];

/// Report Agent - ReACT-pattern report generator.
pub struct ReportAgent {
    llm: LLMClient,
    zep_tools: ZepToolsService,
    graph_id: String,
    simulation_id: String,
    simulation_requirement: String,
    data_dir: PathBuf,
}

/// Max tool calls per section.
const MAX_TOOL_CALLS_PER_SECTION: usize = 5;
/// Min tool calls per section.
const MIN_TOOL_CALLS_PER_SECTION: usize = 3;
/// Max tool calls per chat.
const MAX_TOOL_CALLS_PER_CHAT: usize = 2;
/// Max ReACT iterations per section.
const MAX_REACT_ITERATIONS: usize = 5;

impl ReportAgent {
    pub fn new(
        llm: LLMClient,
        zep: ZepClient,
        graph_id: &str,
        simulation_id: &str,
        simulation_requirement: &str,
    ) -> Self {
        let zep_tools = ZepToolsService::from_client(zep, llm.clone());

        info!(
            graph_id,
            simulation_id, "ReportAgent initialized"
        );

        Self {
            llm,
            zep_tools,
            graph_id: graph_id.to_string(),
            simulation_id: simulation_id.to_string(),
            simulation_requirement: simulation_requirement.to_string(),
            data_dir: PathBuf::from("data"),
        }
    }

    pub fn with_data_dir(mut self, data_dir: &Path) -> Self {
        self.data_dir = data_dir.to_path_buf();
        self
    }

    /// Build description text for available tools.
    fn get_tools_description(&self) -> String {
        let mut parts = vec!["可用工具：".to_string()];
        for tool in TOOLS {
            let params_desc: Vec<String> = tool
                .params
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect();
            parts.push(format!("- {}: {}", tool.name, tool.description));
            if !params_desc.is_empty() {
                parts.push(format!("  参数: {}", params_desc.join(", ")));
            }
        }
        parts.join("\n")
    }

    /// Execute a tool call and return text result.
    async fn execute_tool(
        &self,
        tool_name: &str,
        parameters: &Value,
        report_context: &str,
    ) -> String {
        info!("Executing tool: {}", tool_name);

        match tool_name {
            "insight_forge" => {
                let query = parameters["query"].as_str().unwrap_or("");
                let ctx = parameters["report_context"]
                    .as_str()
                    .unwrap_or(report_context);
                let result = self
                    .zep_tools
                    .insight_forge(&self.graph_id, query, &self.simulation_requirement, ctx, 5)
                    .await;
                result.to_text()
            }
            "panorama_search" => {
                let query = parameters["query"].as_str().unwrap_or("");
                let include_expired = parameters["include_expired"]
                    .as_bool()
                    .or_else(|| {
                        parameters["include_expired"]
                            .as_str()
                            .map(|s| matches!(s.to_lowercase().as_str(), "true" | "1" | "yes"))
                    })
                    .unwrap_or(true);
                let result = self
                    .zep_tools
                    .panorama_search(&self.graph_id, query, include_expired, 50)
                    .await;
                result.to_text()
            }
            "quick_search" => {
                let query = parameters["query"].as_str().unwrap_or("");
                let limit = parameters["limit"]
                    .as_u64()
                    .map(|v| v as usize)
                    .or_else(|| {
                        parameters["limit"]
                            .as_str()
                            .and_then(|s| s.parse::<usize>().ok())
                    })
                    .unwrap_or(10);
                let result = self.zep_tools.quick_search(&self.graph_id, query, limit).await;
                result.to_text()
            }
            "search_graph" => {
                // Redirect to quick_search for backward compat
                info!("search_graph redirected to quick_search");
                let query = parameters["query"].as_str().unwrap_or("");
                let limit = parameters["limit"]
                    .as_u64()
                    .map(|v| v as usize)
                    .or_else(|| {
                        parameters["limit"]
                            .as_str()
                            .and_then(|s| s.parse::<usize>().ok())
                    })
                    .unwrap_or(10);
                let result = self.zep_tools.quick_search(&self.graph_id, query, limit).await;
                result.to_text()
            }
            "get_graph_statistics" => {
                let result = self.zep_tools.get_graph_statistics(&self.graph_id).await;
                serde_json::to_string_pretty(&result).unwrap_or_default()
            }
            "get_entity_summary" => {
                let entity_name = parameters["entity_name"].as_str().unwrap_or("");
                let result = self
                    .zep_tools
                    .get_entity_summary(&self.graph_id, entity_name)
                    .await;
                serde_json::to_string_pretty(&result).unwrap_or_default()
            }
            "get_entities_by_type" => {
                let entity_type = parameters["entity_type"].as_str().unwrap_or("");
                let nodes = self
                    .zep_tools
                    .get_entities_by_type(&self.graph_id, entity_type)
                    .await;
                let result: Vec<Value> = nodes.iter().map(|n| n.to_dict()).collect();
                serde_json::to_string_pretty(&result).unwrap_or_default()
            }
            _ => {
                format!(
                    "未知工具: {}。请使用以下工具之一: insight_forge, panorama_search, quick_search",
                    tool_name
                )
            }
        }
    }

    /// Parse tool calls from LLM response.
    ///
    /// Supports:
    /// 1. `<tool_call>{"name":"...", "parameters":{...}}</tool_call>`
    /// 2. Bare JSON as a fallback
    fn parse_tool_calls(response: &str) -> Vec<Value> {
        let mut tool_calls = Vec::new();

        // Format 1: XML-style
        let xml_re = Regex::new(r"<tool_call>\s*(\{.*?\})\s*</tool_call>").unwrap();
        for cap in xml_re.captures_iter(response) {
            if let Some(m) = cap.get(1) {
                if let Ok(mut data) = serde_json::from_str::<Value>(m.as_str()) {
                    if Self::is_valid_tool_call(&mut data) {
                        tool_calls.push(data);
                    }
                }
            }
        }
        if !tool_calls.is_empty() {
            return tool_calls;
        }

        // Format 2: Bare JSON fallback
        let stripped = response.trim();
        if stripped.starts_with('{') && stripped.ends_with('}') {
            if let Ok(mut data) = serde_json::from_str::<Value>(stripped) {
                if Self::is_valid_tool_call(&mut data) {
                    tool_calls.push(data);
                    return tool_calls;
                }
            }
        }

        // Try to extract last JSON object from response
        let json_re = Regex::new(r#"(\{"(?:name|tool)"\s*:.*?\})\s*$"#).unwrap();
        if let Some(cap) = json_re.captures(stripped) {
            if let Some(m) = cap.get(1) {
                if let Ok(mut data) = serde_json::from_str::<Value>(m.as_str()) {
                    if Self::is_valid_tool_call(&mut data) {
                        tool_calls.push(data);
                    }
                }
            }
        }

        tool_calls
    }

    /// Validate and normalize a parsed tool call JSON.
    fn is_valid_tool_call(data: &mut Value) -> bool {
        let tool_name = data["name"]
            .as_str()
            .or_else(|| data["tool"].as_str())
            .map(|s| s.to_string());

        if let Some(ref name) = tool_name {
            if VALID_TOOL_NAMES.contains(&name.as_str()) {
                // Normalize keys
                if data.get("tool").is_some() {
                    let n = data["tool"].clone();
                    data["name"] = n;
                }
                if data.get("params").is_some() && data.get("parameters").is_none() {
                    let p = data["params"].clone();
                    data["parameters"] = p;
                }
                return true;
            }
        }
        false
    }

    // ═══════════════════════════════════════════════════════════════
    // Plan Outline
    // ═══════════════════════════════════════════════════════════════

    /// Plan report outline using LLM.
    pub async fn plan_outline(
        &self,
        progress_callback: Option<&(dyn Fn(&str, i32, &str) + Send + Sync)>,
    ) -> ReportOutline {
        info!("Planning report outline...");

        if let Some(cb) = progress_callback {
            cb("planning", 0, "正在分析模拟需求...");
        }

        // Get simulation context
        let context = self
            .zep_tools
            .get_simulation_context(&self.graph_id, &self.simulation_requirement, 30)
            .await;

        if let Some(cb) = progress_callback {
            cb("planning", 30, "正在生成报告大纲...");
        }

        let stats = &context["graph_statistics"];
        let total_nodes = stats["total_nodes"].as_u64().unwrap_or(0) as usize;
        let total_edges = stats["total_edges"].as_u64().unwrap_or(0) as usize;
        let entity_types = if let Some(et) = stats["entity_types"].as_object() {
            let keys: Vec<&String> = et.keys().collect();
            format!("{:?}", keys)
        } else {
            "[]".to_string()
        };
        let total_entities = context["total_entities"].as_u64().unwrap_or(0) as usize;

        let related_facts = context["related_facts"]
            .as_array()
            .map(|arr| {
                let limited: Vec<&Value> = arr.iter().take(10).collect();
                serde_json::to_string_pretty(&limited).unwrap_or_default()
            })
            .unwrap_or_else(|| "[]".to_string());

        let user_prompt = build_plan_user_prompt(
            &self.simulation_requirement,
            total_nodes,
            total_edges,
            &entity_types,
            total_entities,
            &related_facts,
        );

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: PLAN_SYSTEM_PROMPT.to_string(),
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt,
            },
        ];

        match self.llm.chat_json(&messages, 0.3, 2048).await {
            Ok(response) => {
                if let Some(cb) = progress_callback {
                    cb("planning", 80, "正在解析大纲结构...");
                }

                let mut sections = Vec::new();
                if let Some(secs) = response["sections"].as_array() {
                    for s in secs {
                        let title = s["title"].as_str().unwrap_or("").to_string();
                        sections.push(ReportSection::new(&title));
                    }
                }

                let outline = ReportOutline {
                    title: response["title"]
                        .as_str()
                        .unwrap_or("模拟分析报告")
                        .to_string(),
                    summary: response["summary"].as_str().unwrap_or("").to_string(),
                    sections,
                };

                if let Some(cb) = progress_callback {
                    cb("planning", 100, "大纲规划完成");
                }

                info!("Outline planned: {} sections", outline.sections.len());
                outline
            }
            Err(e) => {
                error!("Outline planning failed: {}", e);
                // Fallback outline
                ReportOutline {
                    title: "未来预测报告".to_string(),
                    summary: "基于模拟预测的未来趋势与风险分析".to_string(),
                    sections: vec![
                        ReportSection::new("预测场景与核心发现"),
                        ReportSection::new("人群行为预测分析"),
                        ReportSection::new("趋势展望与风险提示"),
                    ],
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Generate Section (ReACT loop)
    // ═══════════════════════════════════════════════════════════════

    /// Generate a single section using the ReACT pattern.
    async fn generate_section_react(
        &self,
        section: &ReportSection,
        outline: &ReportOutline,
        previous_sections: &[String],
        report_logger: Option<&ReportLogger>,
        console_logger: Option<&ReportConsoleLogger>,
        progress_callback: Option<&(dyn Fn(&str, i32, &str) + Send + Sync)>,
        section_index: usize,
    ) -> String {
        info!("ReACT generating section: {}", section.title);

        if let Some(rl) = report_logger {
            rl.log_section_start(&section.title, section_index).await;
        }
        if let Some(cl) = console_logger {
            cl.info(&format!("开始生成章节: {}", section.title)).await;
        }

        let system_prompt = build_section_system_prompt(
            &outline.title,
            &outline.summary,
            &self.simulation_requirement,
            &section.title,
            &self.get_tools_description(),
        );

        // Build previous content summary
        let previous_content = if previous_sections.is_empty() {
            "（这是第一个章节）".to_string()
        } else {
            let parts: Vec<String> = previous_sections
                .iter()
                .map(|s| {
                    if s.len() > 4000 {
                        format!("{}...", &s[..4000])
                    } else {
                        s.clone()
                    }
                })
                .collect();
            parts.join("\n\n---\n\n")
        };

        let user_prompt = build_section_user_prompt(&previous_content, &section.title);

        let mut messages = vec![
            ChatMessage {
                role: "system".into(),
                content: system_prompt,
            },
            ChatMessage {
                role: "user".into(),
                content: user_prompt,
            },
        ];

        let mut tool_calls_count = 0usize;
        let mut conflict_retries = 0usize;
        let mut used_tools: HashSet<String> = HashSet::new();
        let all_tools: HashSet<String> = VALID_TOOL_NAMES.iter().map(|s| s.to_string()).collect();

        let report_context = format!(
            "章节标题: {}\n模拟需求: {}",
            section.title, self.simulation_requirement
        );

        for iteration in 0..MAX_REACT_ITERATIONS {
            if let Some(cb) = progress_callback {
                let prog = ((iteration as f64 / MAX_REACT_ITERATIONS as f64) * 100.0) as i32;
                cb(
                    "generating",
                    prog,
                    &format!(
                        "深度检索与撰写中 ({}/{})",
                        tool_calls_count, MAX_TOOL_CALLS_PER_SECTION
                    ),
                );
            }

            // Call LLM
            let response = match self.llm.chat(&messages, 0.5, 4096, None).await {
                Ok(r) => r,
                Err(e) => {
                    warn!("LLM returned error in section {}: {}", section.title, e);
                    if iteration < MAX_REACT_ITERATIONS - 1 {
                        messages.push(ChatMessage {
                            role: "assistant".into(),
                            content: "（响应为空）".into(),
                        });
                        messages.push(ChatMessage {
                            role: "user".into(),
                            content: "请继续生成内容。".into(),
                        });
                        continue;
                    }
                    break;
                }
            };

            if response.is_empty() {
                warn!("LLM returned empty response for section {}", section.title);
                if iteration < MAX_REACT_ITERATIONS - 1 {
                    messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: "（响应为空）".into(),
                    });
                    messages.push(ChatMessage {
                        role: "user".into(),
                        content: "请继续生成内容。".into(),
                    });
                    continue;
                }
                break;
            }

            debug!("LLM response: {}...", &response[..response.len().min(200)]);

            let mut parsed_tool_calls = Self::parse_tool_calls(&response);
            let mut has_tool_calls = !parsed_tool_calls.is_empty();
            let mut has_final_answer = response.contains("Final Answer:");

            // ── Conflict handling: both tool call and Final Answer ──
            if has_tool_calls && has_final_answer {
                conflict_retries += 1;
                warn!(
                    "Section {} iteration {}: tool call + Final Answer conflict #{}",
                    section.title,
                    iteration + 1,
                    conflict_retries
                );

                if conflict_retries <= 2 {
                    messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: response,
                    });
                    messages.push(ChatMessage {
                        role: "user".into(),
                        content: "【格式错误】你在一次回复中同时包含了工具调用和 Final Answer，这是不允许的。\n\
                            每次回复只能做以下两件事之一：\n\
                            - 调用一个工具（输出一个 <tool_call> 块，不要写 Final Answer）\n\
                            - 输出最终内容（以 'Final Answer:' 开头，不要包含 <tool_call>）\n\
                            请重新回复，只做其中一件事。".into(),
                    });
                    continue;
                } else {
                    // Degrade: truncate to first tool call
                    warn!("Degrading: truncating to first tool call");
                    if let Some(end_pos) = response.find("</tool_call>") {
                        let truncated = &response[..end_pos + "</tool_call>".len()];
                        parsed_tool_calls = Self::parse_tool_calls(truncated);
                        has_tool_calls = !parsed_tool_calls.is_empty();
                    }
                    has_final_answer = false;
                    conflict_retries = 0;
                }
            }

            // Log LLM response
            if let Some(rl) = report_logger {
                rl.log_llm_response(
                    &section.title,
                    section_index,
                    &response,
                    iteration + 1,
                    has_tool_calls,
                    has_final_answer,
                )
                .await;
            }

            // ── Case 1: Final Answer ──
            if has_final_answer {
                if tool_calls_count < MIN_TOOL_CALLS_PER_SECTION {
                    // Not enough tool calls, ask for more
                    messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: response,
                    });
                    let unused: Vec<String> =
                        all_tools.difference(&used_tools).cloned().collect();
                    let unused_hint = if unused.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "（这些工具还未使用，推荐用一下他们: {}）",
                            unused.join(", ")
                        )
                    };
                    messages.push(ChatMessage {
                        role: "user".into(),
                        content: format!(
                            "【注意】你只调用了{}次工具，至少需要{}次。\
                            请再调用工具获取更多模拟数据，然后再输出 Final Answer。{}",
                            tool_calls_count, MIN_TOOL_CALLS_PER_SECTION, unused_hint
                        ),
                    });
                    continue;
                }

                // Accept Final Answer
                let final_answer = response
                    .split("Final Answer:")
                    .last()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                info!(
                    "Section {} complete (tool calls: {})",
                    section.title, tool_calls_count
                );

                if let Some(rl) = report_logger {
                    rl.log_section_content(
                        &section.title,
                        section_index,
                        &final_answer,
                        tool_calls_count,
                    )
                    .await;
                }
                if let Some(cl) = console_logger {
                    cl.info(&format!(
                        "章节 {} 生成完成 (工具调用: {}次)",
                        section.title, tool_calls_count
                    ))
                    .await;
                }
                return final_answer;
            }

            // ── Case 2: Tool calls ──
            if has_tool_calls {
                if tool_calls_count >= MAX_TOOL_CALLS_PER_SECTION {
                    // Over limit, ask for Final Answer
                    messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: response,
                    });
                    messages.push(ChatMessage {
                        role: "user".into(),
                        content: format!(
                            "工具调用次数已达上限（{}/{}），不能再调用工具。\
                            请立即基于已获取的信息，以 \"Final Answer:\" 开头输出章节内容。",
                            tool_calls_count, MAX_TOOL_CALLS_PER_SECTION
                        ),
                    });
                    continue;
                }

                // Execute first tool call only
                let call = &parsed_tool_calls[0];
                if parsed_tool_calls.len() > 1 {
                    info!(
                        "LLM tried {} tool calls, executing only first: {}",
                        parsed_tool_calls.len(),
                        call["name"].as_str().unwrap_or("?")
                    );
                }

                let tool_name = call["name"].as_str().unwrap_or("");
                let params = call.get("parameters").cloned().unwrap_or(json!({}));

                if let Some(rl) = report_logger {
                    rl.log_tool_call(
                        &section.title,
                        section_index,
                        tool_name,
                        &params,
                        iteration + 1,
                    )
                    .await;
                }
                if let Some(cl) = console_logger {
                    cl.info(&format!("调用工具: {}", tool_name)).await;
                }

                let result = self
                    .execute_tool(tool_name, &params, &report_context)
                    .await;

                if let Some(rl) = report_logger {
                    rl.log_tool_result(
                        &section.title,
                        section_index,
                        tool_name,
                        &result,
                        iteration + 1,
                    )
                    .await;
                }

                tool_calls_count += 1;
                used_tools.insert(tool_name.to_string());

                // Build unused tools hint
                let unused: Vec<String> = all_tools.difference(&used_tools).cloned().collect();
                let unused_hint =
                    if !unused.is_empty() && tool_calls_count < MAX_TOOL_CALLS_PER_SECTION {
                        format!(
                            "\n你还没有使用过: {}，建议尝试不同工具获取多角度信息",
                            unused.join("、")
                        )
                    } else {
                        String::new()
                    };

                messages.push(ChatMessage {
                    role: "assistant".into(),
                    content: response,
                });
                messages.push(ChatMessage {
                    role: "user".into(),
                    content: format!(
                        "Observation（检索结果）:\n\n\
                        ═══ 工具 {} 返回 ═══\n\
                        {}\n\n\
                        ═══════════════════════════════════════════════════════════════\n\
                        已调用工具 {}/{} 次（已用: {}）{}\n\
                        - 如果信息充分：以 \"Final Answer:\" 开头输出章节内容（必须引用上述原文）\n\
                        - 如果需要更多信息：调用一个工具继续检索\n\
                        ═══════════════════════════════════════════════════════════════",
                        tool_name,
                        result,
                        tool_calls_count,
                        MAX_TOOL_CALLS_PER_SECTION,
                        used_tools.iter().cloned().collect::<Vec<_>>().join(", "),
                        unused_hint
                    ),
                });
                continue;
            }

            // ── Case 3: Neither tool call nor Final Answer ──
            messages.push(ChatMessage {
                role: "assistant".into(),
                content: response.clone(),
            });

            if tool_calls_count < MIN_TOOL_CALLS_PER_SECTION {
                let unused: Vec<String> = all_tools.difference(&used_tools).cloned().collect();
                let unused_hint = if unused.is_empty() {
                    String::new()
                } else {
                    format!(
                        "（这些工具还未使用，推荐用一下他们: {}）",
                        unused.join(", ")
                    )
                };
                messages.push(ChatMessage {
                    role: "user".into(),
                    content: format!(
                        "当前只调用了 {} 次工具，至少需要 {} 次。\
                        请调用工具获取模拟数据。{}",
                        tool_calls_count, MIN_TOOL_CALLS_PER_SECTION, unused_hint
                    ),
                });
                continue;
            }

            // Enough tool calls, accept response as final content
            info!(
                "Section {} no 'Final Answer:' prefix, accepting LLM output (tool calls: {})",
                section.title, tool_calls_count
            );
            let final_answer = response.trim().to_string();

            if let Some(rl) = report_logger {
                rl.log_section_content(
                    &section.title,
                    section_index,
                    &final_answer,
                    tool_calls_count,
                )
                .await;
            }
            return final_answer;
        }

        // Reached max iterations, force final content
        warn!(
            "Section {} reached max iterations, forcing generation",
            section.title
        );
        messages.push(ChatMessage {
            role: "user".into(),
            content: "已达到工具调用限制，请直接输出 Final Answer: 并生成章节内容。".into(),
        });

        let final_response = self.llm.chat(&messages, 0.5, 4096, None).await;

        let final_answer = match final_response {
            Ok(resp) => {
                if resp.contains("Final Answer:") {
                    resp.split("Final Answer:").last().unwrap_or("").trim().to_string()
                } else {
                    resp
                }
            }
            Err(e) => {
                error!("Section {} forced finish LLM error: {}", section.title, e);
                format!("（本章节生成失败：LLM 返回空响应，请稍后重试）")
            }
        };

        if let Some(rl) = report_logger {
            rl.log_section_content(
                &section.title,
                section_index,
                &final_answer,
                tool_calls_count,
            )
            .await;
        }

        final_answer
    }

    // ═══════════════════════════════════════════════════════════════
    // Generate Full Report
    // ═══════════════════════════════════════════════════════════════

    /// Generate a complete report with section-by-section output.
    pub async fn generate_report(
        &self,
        report_id: Option<&str>,
        progress_callback: Option<&(dyn Fn(&str, i32, &str) + Send + Sync)>,
    ) -> anyhow::Result<Report> {
        let report_id = report_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                format!(
                    "report_{}",
                    &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]
                )
            });

        let start_time = Local::now();

        let mut report = Report::new(&self.simulation_id, Some(&report_id));
        report.status = ReportStatus::Generating;

        let report_manager = ReportManager::new(&self.data_dir);
        let mut completed_section_titles: Vec<String> = Vec::new();

        // Initialize loggers
        let report_logger = ReportLogger::new(&self.data_dir, &report_id).await;
        let console_logger = ReportConsoleLogger::new(&self.data_dir, &report_id).await;

        report_logger
            .log_start(
                &self.simulation_id,
                &self.graph_id,
                &self.simulation_requirement,
            )
            .await;
        console_logger.info("报告生成任务开始").await;

        // Save initial state
        report_manager.save_report(&report).await.ok();

        // Phase 1: Plan outline
        if let Some(cb) = progress_callback {
            cb("planning", 5, "开始规划报告大纲...");
        }
        console_logger.info("开始规划报告大纲...").await;
        report_logger.log_planning_start().await;

        let outline = self
            .plan_outline(progress_callback.map(|cb| {
                // We can't easily transform the callback, so we pass it through
                cb
            }))
            .await;

        report.outline = Some(serde_json::to_value(outline.to_dict())?);
        report_logger.log_planning_complete(&outline.to_dict()).await;
        console_logger
            .info(&format!(
                "大纲规划完成，共{}个章节",
                outline.sections.len()
            ))
            .await;

        // Save outline
        let outline_json = serde_json::to_string_pretty(&outline.to_dict())?;
        let outline_path = self
            .data_dir
            .join("reports")
            .join(&report_id)
            .join("outline.json");
        fs::create_dir_all(outline_path.parent().unwrap()).await?;
        fs::write(&outline_path, &outline_json).await?;

        report_manager.save_report(&report).await.ok();

        if let Some(cb) = progress_callback {
            cb(
                "planning",
                15,
                &format!("大纲规划完成，共{}个章节", outline.sections.len()),
            );
        }

        // Phase 2: Generate sections
        let total_sections = outline.sections.len();
        let mut generated_sections: Vec<String> = Vec::new();

        for (i, section) in outline.sections.iter().enumerate() {
            let section_num = i + 1;
            let base_progress = 20 + ((i as f64 / total_sections as f64) * 70.0) as i32;

            if let Some(cb) = progress_callback {
                cb(
                    "generating",
                    base_progress,
                    &format!(
                        "正在生成章节: {} ({}/{})",
                        section.title, section_num, total_sections
                    ),
                );
            }
            console_logger
                .info(&format!(
                    "正在生成章节: {} ({}/{})",
                    section.title, section_num, total_sections
                ))
                .await;

            let section_content = self
                .generate_section_react(
                    section,
                    &outline,
                    &generated_sections,
                    Some(&report_logger),
                    Some(&console_logger),
                    progress_callback,
                    section_num,
                )
                .await;

            // Clean and save section
            let cleaned_content = clean_section_content(&section_content, &section.title);
            let md_content = format!("## {}\n\n{}\n\n", section.title, cleaned_content);

            report_manager
                .save_section(&report_id, section_num, &md_content)
                .await
                .ok();

            generated_sections.push(format!("## {}\n\n{}", section.title, section_content));
            completed_section_titles.push(section.title.clone());

            // Log section completion
            let full_section = format!("## {}\n\n{}", section.title, section_content);
            report_logger
                .log_section_full_complete(&section.title, section_num, full_section.trim())
                .await;
            console_logger
                .info(&format!("章节 {} 已完成", section.title))
                .await;

            info!("Section saved: {}/section_{:02}.md", report_id, section_num);
        }

        // Phase 3: Assemble full report
        if let Some(cb) = progress_callback {
            cb("generating", 95, "正在组装完整报告...");
        }
        console_logger.info("正在组装完整报告...").await;

        let full_markdown = assemble_full_report(&outline, &generated_sections);
        let full_markdown = post_process_report(&full_markdown, &outline);

        // Save full report markdown
        let full_path = self
            .data_dir
            .join("reports")
            .join(&report_id)
            .join("full_report.md");
        fs::write(&full_path, &full_markdown).await?;

        report.markdown_content = full_markdown;
        report.status = ReportStatus::Completed;
        report.completed_at = Some(Local::now().to_rfc3339());

        let total_time = Local::now()
            .signed_duration_since(start_time)
            .num_milliseconds() as f64
            / 1000.0;

        report_logger
            .log_report_complete(total_sections, total_time)
            .await;
        console_logger.info("报告生成完成").await;

        report_manager.save_report(&report).await.ok();

        if let Some(cb) = progress_callback {
            cb("completed", 100, "报告生成完成");
        }

        info!("Report generation complete: {}", report_id);

        Ok(report)
    }

    // ═══════════════════════════════════════════════════════════════
    // Chat
    // ═══════════════════════════════════════════════════════════════

    /// Chat with the report agent. Supports tool calls in responses.
    pub async fn chat(
        &self,
        message: &str,
        chat_history: &[Value],
    ) -> anyhow::Result<Value> {
        info!("Report Agent chat: {}...", &message[..message.len().min(50)]);

        // Get existing report content
        let report_manager = ReportManager::new(&self.data_dir);
        let report_content = if let Some(report) =
            report_manager.get_report_by_simulation(&self.simulation_id).await
        {
            let mc = &report.markdown_content;
            if mc.len() > 15000 {
                format!("{}\n\n... [报告内容已截断] ...", &mc[..15000])
            } else {
                mc.clone()
            }
        } else {
            "（暂无报告）".to_string()
        };

        let system_prompt = build_chat_system_prompt(
            &self.simulation_requirement,
            &report_content,
            &self.get_tools_description(),
        );

        let mut messages = vec![ChatMessage {
            role: "system".into(),
            content: system_prompt,
        }];

        // Add chat history (last 10 messages)
        let history_slice = if chat_history.len() > 10 {
            &chat_history[chat_history.len() - 10..]
        } else {
            chat_history
        };
        for h in history_slice {
            if let (Some(role), Some(content)) = (h["role"].as_str(), h["content"].as_str()) {
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                });
            }
        }

        messages.push(ChatMessage {
            role: "user".into(),
            content: message.to_string(),
        });

        // ReACT loop (simplified for chat)
        let mut tool_calls_made: Vec<Value> = Vec::new();
        let max_chat_iterations = 2;

        for _iteration in 0..max_chat_iterations {
            let response = self.llm.chat(&messages, 0.5, 4096, None).await?;

            let tool_calls = Self::parse_tool_calls(&response);

            if tool_calls.is_empty() {
                // No tool calls, return response
                let clean = clean_tool_call_tags(&response);
                return Ok(json!({
                    "response": clean.trim(),
                    "tool_calls": tool_calls_made,
                    "sources": tool_calls_made.iter()
                        .filter_map(|tc| tc["parameters"]["query"].as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                }));
            }

            // Execute tool calls (limited)
            let mut tool_results = Vec::new();
            for call in tool_calls.iter().take(1) {
                if tool_calls_made.len() >= MAX_TOOL_CALLS_PER_CHAT {
                    break;
                }
                let tool_name = call["name"].as_str().unwrap_or("");
                let params = call.get("parameters").cloned().unwrap_or(json!({}));
                let result = self.execute_tool(tool_name, &params, "").await;
                let truncated = if result.len() > 1500 {
                    result[..1500].to_string()
                } else {
                    result
                };
                tool_results.push(json!({
                    "tool": tool_name,
                    "result": truncated
                }));
                tool_calls_made.push(call.clone());
            }

            messages.push(ChatMessage {
                role: "assistant".into(),
                content: response,
            });

            let observation = tool_results
                .iter()
                .map(|r| {
                    format!(
                        "[{}结果]\n{}",
                        r["tool"].as_str().unwrap_or("?"),
                        r["result"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            messages.push(ChatMessage {
                role: "user".into(),
                content: format!("{}\n\n请简洁回答问题。", observation),
            });
        }

        // Max iterations reached
        let final_response = self.llm.chat(&messages, 0.5, 4096, None).await?;
        let clean = clean_tool_call_tags(&final_response);

        Ok(json!({
            "response": clean.trim(),
            "tool_calls": tool_calls_made,
            "sources": tool_calls_made.iter()
                .filter_map(|tc| tc["parameters"]["query"].as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        }))
    }
}

// ═══════════════════════════════════════════════════════════════
// Content cleaning utilities
// ═══════════════════════════════════════════════════════════════

/// Clean section content: remove duplicate headings, convert markdown headings to bold.
fn clean_section_content(content: &str, section_title: &str) -> String {
    if content.is_empty() {
        return content.to_string();
    }

    let content = content.trim();
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut cleaned = Vec::new();
    let mut skip_next_empty = false;

    for (i, line) in lines.iter().enumerate() {
        let stripped = line.trim();

        if let Some(caps) = heading_re.captures(stripped) {
            let title_text = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();

            // Skip duplicate section title in first 5 lines
            if i < 5 {
                let t1 = title_text.replace(' ', "");
                let t2 = section_title.replace(' ', "");
                if title_text == section_title || t1 == t2 {
                    skip_next_empty = true;
                    continue;
                }
            }

            // Convert any heading to bold
            cleaned.push(format!("**{}**", title_text));
            cleaned.push(String::new());
            continue;
        }

        if skip_next_empty && stripped.is_empty() {
            skip_next_empty = false;
            continue;
        }

        skip_next_empty = false;
        cleaned.push(line.to_string());
    }

    // Remove leading empty lines
    while !cleaned.is_empty() && cleaned[0].trim().is_empty() {
        cleaned.remove(0);
    }

    // Remove leading separator lines
    while !cleaned.is_empty() && matches!(cleaned[0].trim(), "---" | "***" | "___") {
        cleaned.remove(0);
        while !cleaned.is_empty() && cleaned[0].trim().is_empty() {
            cleaned.remove(0);
        }
    }

    cleaned.join("\n")
}

/// Assemble the full report markdown from outline and generated sections.
fn assemble_full_report(outline: &ReportOutline, generated_sections: &[String]) -> String {
    let mut md = format!("# {}\n\n", outline.title);
    md += &format!("> {}\n\n", outline.summary);
    md += "---\n\n";

    for section_content in generated_sections {
        md += section_content;
        md += "\n\n";
    }

    md
}

/// Post-process the report: remove duplicate headings, convert sub-headings to bold.
fn post_process_report(content: &str, outline: &ReportOutline) -> String {
    let heading_re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let mut processed = Vec::new();
    let mut prev_was_heading = false;

    let section_titles: HashSet<String> = outline.sections.iter().map(|s| s.title.clone()).collect();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let stripped = line.trim();

        if let Some(caps) = heading_re.captures(stripped) {
            let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(0);
            let title = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim();

            // Check for duplicate title in recent processed lines
            let is_duplicate = processed
                .iter()
                .rev()
                .take(5)
                .any(|pl: &String| {
                    if let Some(pc) = heading_re.captures(pl.trim()) {
                        pc.get(2).map(|m| m.as_str().trim()) == Some(title)
                    } else {
                        false
                    }
                });

            if is_duplicate {
                i += 1;
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                continue;
            }

            match level {
                1 => {
                    if title == outline.title {
                        processed.push(line.to_string());
                        prev_was_heading = true;
                    } else if section_titles.contains(title) {
                        processed.push(format!("## {}", title));
                        prev_was_heading = true;
                    } else {
                        processed.push(format!("**{}**", title));
                        processed.push(String::new());
                        prev_was_heading = false;
                    }
                }
                2 => {
                    if section_titles.contains(title) || title == outline.title {
                        processed.push(line.to_string());
                        prev_was_heading = true;
                    } else {
                        processed.push(format!("**{}**", title));
                        processed.push(String::new());
                        prev_was_heading = false;
                    }
                }
                _ => {
                    // ### and below -> bold
                    processed.push(format!("**{}**", title));
                    processed.push(String::new());
                    prev_was_heading = false;
                }
            }
            i += 1;
            continue;
        }

        if stripped == "---" && prev_was_heading {
            i += 1;
            continue;
        }

        if stripped.is_empty() && prev_was_heading {
            if !processed.is_empty() && !processed.last().unwrap().trim().is_empty() {
                processed.push(line.to_string());
            }
            prev_was_heading = false;
        } else {
            processed.push(line.to_string());
            prev_was_heading = false;
        }

        i += 1;
    }

    // Clean consecutive empty lines (max 2)
    let mut result = Vec::new();
    let mut empty_count = 0;
    for line in processed {
        if line.trim().is_empty() {
            empty_count += 1;
            if empty_count <= 2 {
                result.push(line);
            }
        } else {
            empty_count = 0;
            result.push(line);
        }
    }

    result.join("\n")
}

/// Remove tool call XML tags from a response string.
fn clean_tool_call_tags(response: &str) -> String {
    let re1 = Regex::new(r"(?s)<tool_call>.*?</tool_call>").unwrap();
    let cleaned = re1.replace_all(response, "");
    let re2 = Regex::new(r"\[TOOL_CALL\].*?\)").unwrap();
    re2.replace_all(&cleaned, "").to_string()
}
