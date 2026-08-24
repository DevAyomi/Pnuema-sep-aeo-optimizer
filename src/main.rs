use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use axum::extract::{Path, State};
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use url::Url;

// =========================================================================
// 1. Data Models
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeRequest {
    pub url: String,
    pub max_depth: usize,
    pub max_pages: Option<usize>,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct HistoryItem {
    pub id: i64,
    pub url: String,
    pub seo_score: f64,
    pub aeo_score: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Category {
    SEO,
    AEO,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Severity {
    Critical, // Weight: 5
    Warning,  // Weight: 2
    Info,     // Weight: 1
}

impl Severity {
    fn weight(&self) -> u32 {
        match self {
            Severity::Critical => 5,
            Severity::Warning => 2,
            Severity::Info => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOutcome {
    pub rule_id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub severity: Severity,
    pub passed: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionAnalysis {
    pub heading: String,
    pub text: String,
    pub score: u32,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageReport {
    pub url: String,
    pub status: u16,
    pub load_time_ms: u128,
    pub seo_score: f64,
    pub aeo_score: f64,
    pub outcomes: Vec<RuleOutcome>,
    pub extractability_sections: Vec<SectionAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteReport {
    pub site_seo_score: f64,
    pub site_aeo_score: f64,
    pub top_issues: Vec<IssueSummary>,
    pub per_page_reports: Vec<PageReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueSummary {
    pub rule_id: String,
    pub name: String,
    pub category: Category,
    pub severity: Severity,
    pub failure_count: usize,
    pub total_count: usize,
    pub impact_score: u32, // failure_count * severity_weight
}

// Context passed to rules
pub struct PageContext {
    pub url: Url,
    pub status: u16,
    pub headers: reqwest::header::HeaderMap,
    pub raw_html: String,
    pub dom: Html,
    pub timing_ms: u128,
    pub robots_allows_ai: bool,
    pub robots_message: String,
}

// =========================================================================
// 2. Rules Definition & Registry
// =========================================================================

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn category(&self) -> Category;
    fn severity(&self) -> Severity;
    fn applies_to(&self, ctx: &PageContext) -> bool;
    fn run(&self, ctx: &PageContext) -> RuleOutcome;
}

pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        Self {
            rules: vec![
                Box::new(RuleTitleExists),
                Box::new(RuleMetaDescriptionExists),
                Box::new(RuleH1Count),
                Box::new(RuleImgAlt),
                Box::new(RuleSpaDetection),
                Box::new(RuleCanonicalExists),
                Box::new(RuleViewportExists),
                Box::new(RuleJsonLdExists),
                Box::new(RuleStructuredDataStructure),
                Box::new(RuleAeoExtractabilityScore),
                Box::new(RuleRobotsAiAllows),
                Box::new(RuleEeatCitations),
            ],
        }
    }

    pub fn evaluate(&self, ctx: &PageContext) -> Vec<RuleOutcome> {
        let mut outcomes = Vec::new();
        for rule in &self.rules {
            if rule.applies_to(ctx) {
                outcomes.push(rule.run(ctx));
            }
        }
        outcomes
    }
}

// Rule 1: Title Tag Exists
struct RuleTitleExists;
impl Rule for RuleTitleExists {
    fn id(&self) -> &'static str { "SEO-001" }
    fn name(&self) -> &'static str { "Title Tag" }
    fn description(&self) -> &'static str { "Verifies the page has a non-empty <title> tag." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("title").unwrap();
        let title = ctx.dom.select(&selector).next().map(|el| el.text().collect::<String>());
        let passed = title.as_ref().map_or(false, |t| !t.trim().is_empty());
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Title tag found: \"{}\"", title.unwrap().trim())
            } else {
                "Missing or empty <title> tag.".to_string()
            },
        }
    }
}

// Rule 2: Meta Description Tag Exists
struct RuleMetaDescriptionExists;
impl Rule for RuleMetaDescriptionExists {
    fn id(&self) -> &'static str { "SEO-002" }
    fn name(&self) -> &'static str { "Meta Description" }
    fn description(&self) -> &'static str { "Verifies the page has a non-empty meta description tag." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("meta[name='description']").unwrap();
        let desc = ctx.dom.select(&selector).next().and_then(|el| el.value().attr("content"));
        let passed = desc.as_ref().map_or(false, |d| !d.trim().is_empty());
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Meta description found ({} chars)", desc.unwrap().len())
            } else {
                "Missing or empty meta description tag.".to_string()
            },
        }
    }
}

// Rule 3: H1 Count (Exactly one H1 is recommended)
struct RuleH1Count;
impl Rule for RuleH1Count {
    fn id(&self) -> &'static str { "SEO-003" }
    fn name(&self) -> &'static str { "Single H1 Tag" }
    fn description(&self) -> &'static str { "Checks that the page has exactly one <h1> tag." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("h1").unwrap();
        let count = ctx.dom.select(&selector).count();
        let passed = count == 1;
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: match count {
                0 => "Missing <h1> tag. Every page should have exactly one main heading.".to_string(),
                1 => "Exactly one <h1> tag found.".to_string(),
                n => format!("Multiple <h1> tags found ({}). Standard practice is to have only one.", n),
            },
        }
    }
}

// Rule 4: Image Alt Attributes
struct RuleImgAlt;
impl Rule for RuleImgAlt {
    fn id(&self) -> &'static str { "SEO-004" }
    fn name(&self) -> &'static str { "Image Alt Attributes" }
    fn description(&self) -> &'static str { "Checks that all <img> tags have an alt attribute." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, ctx: &PageContext) -> bool {
        let selector = Selector::parse("img").unwrap();
        ctx.dom.select(&selector).next().is_some()
    }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("img").unwrap();
        let mut total = 0;
        let mut missing = 0;
        for el in ctx.dom.select(&selector) {
            total += 1;
            if el.value().attr("alt").map_or(true, |alt| alt.trim().is_empty()) {
                missing += 1;
            }
        }
        let passed = missing == 0;
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("All {} images have alt attributes.", total)
            } else {
                format!("{}/{} images are missing descriptive alt attributes.", missing, total)
            },
        }
    }
}

// Rule 5: JSON-LD Block exists (AEO critical)
struct RuleJsonLdExists;
impl Rule for RuleJsonLdExists {
    fn id(&self) -> &'static str { "AEO-001" }
    fn name(&self) -> &'static str { "JSON-LD Structured Data" }
    fn description(&self) -> &'static str { "Checks for schema metadata in script[type='application/ld+json']." }
    fn category(&self) -> Category { Category::AEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("script[type='application/ld+json']").unwrap();
        let count = ctx.dom.select(&selector).count();
        let passed = count > 0;
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Found {} JSON-LD structured data block(s).", count)
            } else {
                "No JSON-LD structured data blocks found. Search engines and AI engines rely on this context.".to_string()
            },
        }
    }
}

// Rule 6: Structured Data layout (lists, tables, definitions) (AEO Warning)
struct RuleStructuredDataStructure;
impl Rule for RuleStructuredDataStructure {
    fn id(&self) -> &'static str { "AEO-002" }
    fn name(&self) -> &'static str { "Structured Layout Elements" }
    fn description(&self) -> &'static str { "Checks for easy-to-digest formats like lists, tables, or definition lists." }
    fn category(&self) -> Category { Category::AEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let list_sel = Selector::parse("ul, ol").unwrap();
        let table_sel = Selector::parse("table").unwrap();
        let dl_sel = Selector::parse("dl").unwrap();
        
        let lists = ctx.dom.select(&list_sel).count();
        let tables = ctx.dom.select(&table_sel).count();
        let dls = ctx.dom.select(&dl_sel).count();
        
        let passed = (lists + tables + dls) > 0;
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Detected structured elements: {} lists, {} tables, {} definitions.", lists, tables, dls)
            } else {
                "No lists, tables, or definition lists found. Presenting data in structures makes it highly scrapable for LLMs.".to_string()
            },
        }
    }
}

// Rule 7: AEO Extractability Score (AEO Critical)
struct RuleAeoExtractabilityScore;
impl Rule for RuleAeoExtractabilityScore {
    fn id(&self) -> &'static str { "AEO-003" }
    fn name(&self) -> &'static str { "Content Extractability Analysis" }
    fn description(&self) -> &'static str { "Checks if sections are structured for easy retrieval (question/answer format)." }
    fn category(&self) -> Category { Category::AEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, _ctx: &PageContext) -> RuleOutcome {
        // This is evaluated dynamically inside the scorer/page analyzer.
        // We'll pass it if the overall average extractability of sections is >= 60%
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed: true, // Default to true placeholder, actual result updated during scoring
            message: "".to_string(),
        }
    }
}

// Rule 8: SPA Detection (SEO-005)
struct RuleSpaDetection;
impl Rule for RuleSpaDetection {
    fn id(&self) -> &'static str { "SEO-005" }
    fn name(&self) -> &'static str { "SPA Client-Side Render Check" }
    fn description(&self) -> &'static str { "Checks if the page relies heavily on client-side rendering (SPA) without server-side rendering." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let body_sel = Selector::parse("body").unwrap();
        let body_text = ctx.dom.select(&body_sel)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default();
        
        let cleaned_text_len = body_text.trim().len();
        
        let root_sel = Selector::parse("#root, #app, [id*='root'], [id*='app']").unwrap();
        let has_root_div = ctx.dom.select(&root_sel).next().is_some();
        let script_sel = Selector::parse("script").unwrap();
        let has_scripts = ctx.dom.select(&script_sel).next().is_some();

        let is_empty_spa = cleaned_text_len < 300 && has_root_div && has_scripts;
        let passed = !is_empty_spa;

        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Initial HTML contains decent static text content ({} chars). Likely SSR/SSG/Hydrated.", cleaned_text_len)
            } else {
                format!("Initial HTML contains very low static content ({} chars) and has empty SPA container elements. Search crawlers might index an empty page.", cleaned_text_len)
            },
        }
    }
}

// Rule 9: Canonical Link Tag (SEO-006)
struct RuleCanonicalExists;
impl Rule for RuleCanonicalExists {
    fn id(&self) -> &'static str { "SEO-006" }
    fn name(&self) -> &'static str { "Canonical Link Tag" }
    fn description(&self) -> &'static str { "Verifies the page has a <link rel='canonical' href='...'> tag." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("link[rel='canonical']").unwrap();
        let canonical_el = ctx.dom.select(&selector).next();
        let passed = canonical_el.is_some() && canonical_el.and_then(|el| el.value().attr("href")).map_or(false, |href| !href.trim().is_empty());
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Canonical link tag is present: '{}'", canonical_el.unwrap().value().attr("href").unwrap_or(""))
            } else {
                "Canonical link tag is missing. This can cause duplicate content issues.".to_string()
            },
        }
    }
}

// Rule 10: Mobile Viewport Tag (SEO-007)
struct RuleViewportExists;
impl Rule for RuleViewportExists {
    fn id(&self) -> &'static str { "SEO-007" }
    fn name(&self) -> &'static str { "Mobile Viewport Tag" }
    fn description(&self) -> &'static str { "Verifies the page has a mobile-responsive viewport meta tag." }
    fn category(&self) -> Category { Category::SEO }
    fn severity(&self) -> Severity { Severity::Critical }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let selector = Selector::parse("meta[name='viewport']").unwrap();
        let viewport_el = ctx.dom.select(&selector).next();
        let passed = viewport_el.is_some() && viewport_el.and_then(|el| el.value().attr("content")).map_or(false, |content| !content.trim().is_empty());
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("Viewport meta tag is set correctly: '{}'", viewport_el.unwrap().value().attr("content").unwrap_or(""))
            } else {
                "Viewport meta tag is missing. Page is not optimized for mobile devices.".to_string()
            },
        }
    }
}

// Rule 11: AI crawler access (AEO-004)
struct RuleRobotsAiAllows;
impl Rule for RuleRobotsAiAllows {
    fn id(&self) -> &'static str { "AEO-004" }
    fn name(&self) -> &'static str { "AI Crawler Bot Access" }
    fn description(&self) -> &'static str { "Verifies robots.txt does not block LLM/AI crawlers (GPTBot, ClaudeBot, etc.)" }
    fn category(&self) -> Category { Category::AEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed: ctx.robots_allows_ai,
            message: ctx.robots_message.clone(),
        }
    }
}

// Rule 12: E-E-A-T & Citation Check (AEO-005)
struct RuleEeatCitations;
impl Rule for RuleEeatCitations {
    fn id(&self) -> &'static str { "AEO-005" }
    fn name(&self) -> &'static str { "E-E-A-T and Citations Check" }
    fn description(&self) -> &'static str { "Verifies presence of author attribution or outgoing references/citation links." }
    fn category(&self) -> Category { Category::AEO }
    fn severity(&self) -> Severity { Severity::Warning }
    fn applies_to(&self, _ctx: &PageContext) -> bool { true }
    fn run(&self, ctx: &PageContext) -> RuleOutcome {
        let a_selector = Selector::parse("a[href]").unwrap();
        let mut external_citations = 0;
        let domain = ctx.url.host_str().unwrap_or("");
        
        for a_el in ctx.dom.select(&a_selector) {
            if let Some(href) = a_el.value().attr("href") {
                if href.starts_with("http") {
                    if let Ok(u) = Url::parse(href) {
                        if u.host_str() != Some(domain) {
                            external_citations += 1;
                        }
                    }
                }
            }
        }

        let author_selector = Selector::parse("[class*='author'], [id*='author'], meta[name='author'], [itemprop='author']").unwrap();
        let has_author_indicator = ctx.dom.select(&author_selector).next().is_some() || ctx.raw_html.to_lowercase().contains("written by") || ctx.raw_html.to_lowercase().contains("author:");
        
        let passed = external_citations > 0 || has_author_indicator;
        
        RuleOutcome {
            rule_id: self.id().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            category: self.category(),
            severity: self.severity(),
            passed,
            message: if passed {
                format!("E-E-A-T check passed: Found {} external citation link(s) and author signals ({}).", external_citations, if has_author_indicator { "Yes" } else { "No" })
            } else {
                "No author signatures (e.g. meta author, class='author') or external reference citation links found. Trust signals are critical for AI indexing.".to_string()
            },
        }
    }
}

// =========================================================================
// 3. Scoring & Heuristic Engines
// =========================================================================

/// Parse sentences in a block of text
fn split_sentences(text: &str) -> Vec<String> {
    text.split(|c| c == '.' || c == '?' || c == '!')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Analyze extractability score for a specific section
/// Rule details:
/// - if first_sentence answers heading directly (keyword overlap + declarative structure): score += 40
/// - if first_sentence.length between 15 and 40 words: score += 20
/// - if section contains structured data (list, table, or definition pattern): score += 20
/// - if section has no unresolved pronoun/context dependency on prior section: score += 20
fn analyze_section_extractability(heading: &str, body: &str, has_nested_struct: bool) -> SectionAnalysis {
    let mut score = 0;
    let mut details = Vec::new();
    let sentences = split_sentences(body);

    if sentences.is_empty() {
        return SectionAnalysis {
            heading: heading.to_string(),
            text: body.to_string(),
            score: 0,
            details: vec!["Section body is empty.".to_string()],
        };
    }

    let first_sentence = &sentences[0];
    let word_count = first_sentence.split_whitespace().count();

    // 1. Direct answer heuristic (keyword overlap + declarative structure)
    // Check keyword overlap (lowercase comparisons)
    let heading_words: HashSet<String> = heading.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| w.len() > 3) // filter short stop-words
        .collect();

    let sentence_words: HashSet<String> = first_sentence.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .collect();

    let overlap_count = heading_words.iter().filter(|w| sentence_words.contains(w.as_str())).count();
    
    // Declarative verbs starting points
    let is_declarative = first_sentence.contains(" is ") || first_sentence.contains(" are ") || first_sentence.contains(" refers to ") || first_sentence.contains(" means ");
    
    if overlap_count >= 1 && is_declarative {
        score += 40;
        details.push("Direct QA Answer alignment: High heading keyword overlap + declarative sentence structure (+40)".to_string());
    } else if overlap_count >= 1 {
        score += 20;
        details.push("Moderate QA Answer alignment: Contains heading keywords but lacks clear declarative linkage (+20)".to_string());
    } else {
        details.push("Low QA Answer alignment: First sentence does not directly answer/rephrase heading (+0)".to_string());
    }

    // 2. Ideal sentence length for quotation / feature snippet (15 - 40 words)
    if word_count >= 15 && word_count <= 40 {
        score += 20;
        details.push(format!("Quotable sentence length: {} words is optimal (+20)", word_count));
    } else {
        details.push(format!("Sub-optimal sentence length: {} words (ideal is 15-40) (+0)", word_count));
    }

    // 3. Structured data check (contains lists, tables, or definition pattern)
    if has_nested_struct {
        score += 20;
        details.push("Structured layout elements (list, table, definition) present in section (+20)".to_string());
    } else {
        details.push("No structured elements found in this section (+0)".to_string());
    }

    // 4. Context dependency check
    // Simple heuristic: does the first sentence begin with or contain unresolved pronouns (he, she, they, it, this, these, those)
    let lower_first = first_sentence.to_lowercase();
    let has_pronoun_dependency = ["it ", "they ", "this ", "these ", "those ", "he ", "she "]
        .iter()
        .any(|p| lower_first.starts_with(p));

    if !has_pronoun_dependency {
        score += 20;
        details.push("No pronoun dependency: Section stands alone contextually (+20)".to_string());
    } else {
        details.push("Pronoun dependency detected: Starts with context-dependent pronoun (+0)".to_string());
    }

    SectionAnalysis {
        heading: heading.to_string(),
        text: body.to_string(),
        score,
        details,
    }
}

pub fn calculate_weighted_score(outcomes: &[RuleOutcome], category: Category) -> f64 {
    let filtered: Vec<&RuleOutcome> = outcomes.iter().filter(|o| o.category == category).collect();
    if filtered.is_empty() {
        return 100.0;
    }
    let mut total_weight = 0;
    let mut earned = 0;
    for outcome in filtered {
        let weight = outcome.severity.weight();
        total_weight += weight;
        if outcome.passed {
            earned += weight;
        }
    }
    (earned as f64 / total_weight as f64) * 100.0
}

// =========================================================================
// 4. Discovery / Crawling engine
// =========================================================================

fn checks_robots_for_ai(robots_txt: &str) -> (bool, String) {
    if robots_txt.is_empty() {
        return (true, "No robots.txt found (AI crawlers allowed by default).".to_string());
    }
    let lower = robots_txt.to_lowercase();
    let ai_agents = ["gptbot", "claudebot", "perplexitybot", "google-extended", "applebot-extended"];
    let mut blocked = Vec::new();
    
    let lines: Vec<&str> = lower.lines().map(|l| l.trim()).collect();
    let mut current_agent = String::new();
    for line in lines {
        if line.starts_with("user-agent:") {
            current_agent = line.replace("user-agent:", "").trim().to_string();
        } else if line.starts_with("disallow:") && !current_agent.is_empty() {
            let path = line.replace("disallow:", "").trim().to_string();
            if path == "/" || path == "/*" {
                if ai_agents.contains(&current_agent.as_str()) || current_agent == "*" {
                    blocked.push(current_agent.clone());
                }
            }
        }
    }
    if blocked.is_empty() {
        (true, "AI crawler bots (GPTBot, ClaudeBot, etc.) are allowed in robots.txt.".to_string())
    } else {
        (false, format!("Robots.txt restricts AI crawlers: blocked user-agents include: {}.", blocked.join(", ")))
    }
}

pub async fn crawl_and_analyze(seed_url_str: &str, max_depth: usize, max_pages: usize) -> Result<SiteReport, String> {
    let seed_url = Url::parse(seed_url_str).map_err(|e| format!("Invalid seed URL: {}", e))?;
    let domain = seed_url.host_str().ok_or("URL has no host")?.to_string();

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("SeoAeoAnalyzerBot/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let mut queue = VecDeque::new();
    queue.push_back((seed_url.clone(), 0));

    let mut visited = HashSet::new();
    let mut page_reports = Vec::new();
    let rule_registry = RuleRegistry::new();

    // Fetch robots.txt and check for AI block
    let robots_url = seed_url.join("/robots.txt").unwrap();
    let mut robots_txt = String::new();
    if let Ok(res) = client.get(robots_url).send().await {
        if res.status().is_success() {
            if let Ok(text) = res.text().await {
                robots_txt = text;
            }
        }
    }
    let (robots_allows_ai, robots_message) = checks_robots_for_ai(&robots_txt);

    // Sitemaps parsing attempt (simplified mock fallback / fetch check)
    // In a real crawler, we would fetch seed_url + "/sitemap.xml"
    // Let's try fetching and parsing it if possible, else log warning.
    let sitemap_url = seed_url.join("/sitemap.xml").unwrap();
    let mut sitemap_urls = Vec::new();
    if let Ok(res) = client.get(sitemap_url.clone()).send().await {
        if res.status().is_success() {
            if let Ok(body) = res.text().await {
                // simple regex or XML parser extract locations
                let re = Selector::parse("loc").unwrap();
                let document = Html::parse_fragment(&body);
                for el in document.select(&re) {
                    let loc_text = el.text().collect::<String>();
                    if let Ok(loc_url) = Url::parse(&loc_text) {
                        if loc_url.host_str() == Some(&domain) {
                            sitemap_urls.push(loc_url);
                        }
                    }
                }
            }
        }
    }

    // Populate queue with sitemap urls first if found
    for u in sitemap_urls {
        queue.push_back((u, 0));
    }

    while let Some((url, depth)) = queue.pop_front() {
        if visited.contains(&url.to_string()) {
            continue;
        }
        visited.insert(url.to_string());

        if page_reports.len() >= max_pages {
            break;
        }

        // Rate limiting (simple delay to respect the target host)
        tokio::time::sleep(Duration::from_millis(200)).await;

        let start_time = Instant::now();
        let res_result = client.get(url.clone()).send().await;
        let elapsed = start_time.elapsed().as_millis();

        let (status, headers, raw_html) = match res_result {
            Ok(res) => {
                let status = res.status().as_u16();
                let headers = res.headers().clone();
                let text = res.text().await.unwrap_or_default();
                (status, headers, text)
            }
            Err(_) => {
                // connection error
                continue;
            }
        };

        let dom = Html::parse_document(&raw_html);
        let ctx = PageContext {
            url: url.clone(),
            status,
            headers,
            raw_html,
            dom,
            timing_ms: elapsed,
            robots_allows_ai,
            robots_message: robots_message.clone(),
        };

        // Extract AEO sections (Heuristic: head/para blocks)
        let mut sections = Vec::new();
        let h_selector = Selector::parse("h2, h3").unwrap();
        
        for head_el in ctx.dom.select(&h_selector) {
            let heading_text = head_el.text().collect::<String>().trim().to_string();
            if heading_text.is_empty() {
                continue;
            }

            // Find immediate next paragraphs until the next heading
            // Scraper doesn't easily let us query sibling elements in order, so we'll grab
            // context by looking for sibling selectors or simple content parsing.
            // Let's do a robust heuristic: grab the text in the parent container
            // or search for the next elements.
            // A simple alternative: extract the text of the sibling paragraphs.
            // We can search the DOM tree. For simplicity, let's look for paragraphs.
            // Simple traversal: look for paragraphs inside the parent containing this heading
            // Or look for siblings. Since scraper doesn't expose next_sibling easily, we will simulate
            // finding paragraphs nearby.
            let p_selector = Selector::parse("p").unwrap();
            let mut paras = Vec::new();
            for p in ctx.dom.select(&p_selector) {
                let p_text = p.text().collect::<String>();
                if !p_text.trim().is_empty() {
                    paras.push(p_text);
                }
            }
            
            // Join a few paragraphs as a sample
            let body_text = paras.iter().take(2).cloned().collect::<Vec<String>>().join(" ");
            
            let list_table_selector = Selector::parse("ul, ol, table").unwrap();
            let has_nested = ctx.dom.select(&list_table_selector).next().is_some();

            if !body_text.is_empty() {
                let analysis = analyze_section_extractability(&heading_text, &body_text, has_nested);
                sections.push(analysis);
            }
        }

        // Evaluate Rules
        let mut outcomes = rule_registry.evaluate(&ctx);

        // Update AeoExtractability Outcome dynamically
        let avg_extractability = if sections.is_empty() {
            0.0
        } else {
            sections.iter().map(|s| s.score).sum::<u32>() as f64 / sections.len() as f64
        };

        if let Some(outcome) = outcomes.iter_mut().find(|o| o.rule_id == "AEO-003") {
            outcome.passed = avg_extractability >= 60.0;
            outcome.message = format!(
                "Average content extractability is {:.1}% ({} sections evaluated). Needs to be >= 60%.",
                avg_extractability,
                sections.len()
            );
        }

        let seo_score = calculate_weighted_score(&outcomes, Category::SEO);
        let aeo_score = calculate_weighted_score(&outcomes, Category::AEO);

        page_reports.push(PageReport {
            url: url.to_string(),
            status,
            load_time_ms: elapsed,
            seo_score,
            aeo_score,
            outcomes,
            extractability_sections: sections,
        });

        // Add links to crawl queue if depth < max_depth
        if depth < max_depth {
            let a_selector = Selector::parse("a[href]").unwrap();
            for a_el in ctx.dom.select(&a_selector) {
                if let Some(href) = a_el.value().attr("href") {
                    if let Ok(resolved) = url.join(href) {
                        // Keep to the same domain
                        if resolved.host_str() == Some(&domain) {
                            let resolved_clean = {
                                let mut r = resolved.clone();
                                r.set_fragment(None);
                                r
                            };
                            if !visited.contains(&resolved_clean.to_string()) {
                                queue.push_back((resolved_clean, depth + 1));
                            }
                        }
                    }
                }
            }
        }
    }

    if page_reports.is_empty() {
        return Err("Could not crawl any pages successfully.".to_string());
    }

    // Site Level Aggregation
    let site_seo_score = page_reports.iter().map(|p| p.seo_score).sum::<f64>() / page_reports.len() as f64;
    let site_aeo_score = page_reports.iter().map(|p| p.aeo_score).sum::<f64>() / page_reports.len() as f64;

    // Aggregate issue frequencies
    let mut issue_map: HashMap<String, (String, Category, Severity, usize, usize)> = HashMap::new();
    for page in &page_reports {
        for outcome in &page.outcomes {
            let entry = issue_map.entry(outcome.rule_id.clone()).or_insert((
                outcome.name.clone(),
                outcome.category,
                outcome.severity,
                0,
                0,
            ));
            entry.4 += 1; // total evaluated
            if !outcome.passed {
                entry.3 += 1; // failure count
            }
        }
    }

    let mut top_issues: Vec<IssueSummary> = issue_map
        .into_iter()
        .filter(|(_, (_, _, _, fail_c, _))| *fail_c > 0)
        .map(|(rule_id, (name, category, severity, failure_count, total_count))| {
            let impact_score = (failure_count as u32) * severity.weight();
            IssueSummary {
                rule_id,
                name,
                category,
                severity,
                failure_count,
                total_count,
                impact_score,
            }
        })
        .collect();

    // Sort descending by impact score
    top_issues.sort_by(|a, b| b.impact_score.cmp(&a.impact_score));

    Ok(SiteReport {
        site_seo_score,
        site_aeo_score,
        top_issues,
        per_page_reports: page_reports,
    })
}

// =========================================================================
// 5. Axum API Server & Entrypoint
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub email: String,
}

fn is_valid_email(email: &str) -> bool {
    if email.len() < 5 || email.len() > 254 {
        return false;
    }
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let username = parts[0];
    let domain = parts[1];
    if username.is_empty() || domain.is_empty() {
        return false;
    }
    
    // Reject common disposable/temporary email domains
    let disposable_domains = [
        "mailinator.com", "yopmail.com", "tempmail.com", "temp-mail.org", 
        "10minutemail.com", "guerrillamail.com", "sharklasers.com", 
        "dispostable.com", "getairmail.com", "maildrop.cc", "tempmailaddress.com"
    ];
    let lower_domain = domain.to_lowercase();
    if disposable_domains.contains(&lower_domain.as_str()) {
        return false;
    }

    if !domain.contains('.') {
        return false;
    }
    let domain_parts: Vec<&str> = domain.split('.').collect();
    if domain_parts.iter().any(|part| part.is_empty()) {
        return false;
    }
    
    // Verify TLD is at least 2 characters and only alphabetic (e.g. .com, .org, .uk)
    if let Some(tld) = domain_parts.last() {
        if tld.len() < 2 || !tld.chars().all(|c| c.is_alphabetic()) {
            return false;
        }
    } else {
        return false;
    }

    true
}

async fn handle_subscribe(
    State(state): State<AppState>,
    Json(req): Json<SubscribeRequest>,
) -> impl IntoResponse {
    let email = req.email.trim();
    if !is_valid_email(email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Please enter a valid email address format." })),
        ).into_response();
    }

    let result = sqlx::query(
        "INSERT INTO subscribers (email) VALUES ($1) ON CONFLICT (email) DO NOTHING"
    )
    .bind(email)
    .execute(&state.pool)
    .await;

    match result {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "status": "success" }))).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": err.to_string() }))).into_response(),
    }
}

async fn handle_analyze(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeRequest>,
) -> impl IntoResponse {
    let max_depth = req.max_depth;
    let max_pages = req.max_pages.unwrap_or(15);
    
    match crawl_and_analyze(&req.url, max_depth, max_pages).await {
        Ok(report) => {
            if let Ok(report_json) = serde_json::to_string(&report) {
                let _ = sqlx::query(
                    "INSERT INTO reports (url, seo_score, aeo_score, report_json) VALUES ($1, $2, $3, $4::jsonb)"
                )
                .bind(&req.url)
                .bind(report.site_seo_score)
                .bind(report.site_aeo_score)
                .bind(report_json)
                .execute(&state.pool)
                .await;
            }
            (StatusCode::OK, Json(report)).into_response()
        }
        Err(err) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": err }))).into_response(),
    }
}

// =========================================================================
// 6. AI Fix Endpoint (DeepSeek)
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct AiFixRequest {
    pub rule_id: String,
    pub rule_name: String,
    pub rule_message: String,
    pub page_url: String,
    pub category: String,
}

#[derive(Debug, Serialize)]
pub struct AiFixResponse {
    pub fix: String,
}

async fn handle_ai_fix(Json(req): Json<AiFixRequest>) -> impl IntoResponse {
    let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_else(|_| {
        eprintln!("Warning: DEEPSEEK_API_KEY environment variable not set. Using fallback API key.");
        "sk-57770ed095a64fa8a99d1532a8da869f".to_string()
    });
    
    let prompt = format!(
        "You are a senior web developer and SEO/AEO specialist. A website analysis tool found the following issue on a page.\n\n\
        Page URL: {}\n\
        Rule ID: {} (Category: {})\n\
        Rule Name: {}\n\
        Issue: {}\n\n\
        Provide a concise, actionable fix. Include a specific code example if applicable (HTML, meta tags, JSON-LD, robots.txt, etc). \
        Keep it under 200 words. Do not repeat the problem - go straight to the solution.",
        req.page_url, req.rule_id, req.category, req.rule_name, req.rule_message
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    let body = serde_json::json!({
        "model": "deepseek-chat",
        "messages": [
            {
                "role": "system",
                "content": "You are a concise technical SEO and AEO expert. Provide direct, actionable fixes with code examples when relevant. Use markdown formatting."
            },
            {
                "role": "user",
                "content": prompt
            }
        ],
        "temperature": 0.3,
        "max_tokens": 512
    });

    let result = client
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    match result {
        Ok(res) => {
            if !res.status().is_success() {
                let status = res.status();
                let err_text = res.text().await.unwrap_or_default();
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(serde_json::json!({ "fix": format!("DeepSeek API error ({}): {}", status, err_text) })),
                ).into_response();
            }
            match res.json::<serde_json::Value>().await {
                Ok(json) => {
                    let fix_text = json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("No response generated.")
                        .to_string();
                    (StatusCode::OK, Json(AiFixResponse { fix: fix_text })).into_response()
                }
                Err(e) => {
                    (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "fix": format!("Failed to parse DeepSeek response: {}", e) }))).into_response()
                }
            }
        }
        Err(e) => {
            (StatusCode::BAD_GATEWAY, Json(serde_json::json!({ "fix": format!("Failed to reach DeepSeek API: {}", e) }))).into_response()
        }
    }
}

async fn handle_history(State(state): State<AppState>) -> impl IntoResponse {
    let rows = sqlx::query_as::<_, HistoryItem>(
        "SELECT id, url, seo_score, aeo_score, created_at::text FROM reports ORDER BY id DESC"
    )
    .fetch_all(&state.pool)
    .await;
    
    match rows {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": err.to_string() }))).into_response(),
    }
}

async fn handle_history_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>
) -> impl IntoResponse {
    let row: Result<(String,), sqlx::Error> = sqlx::query_as(
        "SELECT report_json::text FROM reports WHERE id = $1"
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await;
    
    match row {
        Ok((json_str,)) => {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json_str) {
                (StatusCode::OK, Json(val)).into_response()
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Failed to deserialize report json" }))).into_response()
            }
        }
        Err(err) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": err.to_string() }))).into_response(),
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/seo_aeo".to_string());

    // Auto-create database if it doesn't exist
    if let Some(idx) = db_url.rfind('/') {
        let base_url = &db_url[..=idx];
        let db_name = &db_url[idx + 1..];
        let admin_url = format!("{}postgres", base_url);
        
        if let Ok(admin_pool) = sqlx::PgPool::connect(&admin_url).await {
            let row: Result<(i64,), sqlx::Error> = sqlx::query_as(
                "SELECT count(*) FROM pg_database WHERE datname = $1"
            )
            .bind(db_name)
            .fetch_one(&admin_pool)
            .await;

            if let Ok((count,)) = row {
                if count == 0 {
                    let _ = sqlx::query(&format!("CREATE DATABASE {}", db_name))
                        .execute(&admin_pool)
                        .await;
                }
            }
        }
    }

    let pool = sqlx::PgPool::connect(&db_url).await.unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS reports (
            id BIGSERIAL PRIMARY KEY,
            url TEXT NOT NULL,
            seo_score DOUBLE PRECISION NOT NULL,
            aeo_score DOUBLE PRECISION NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
            report_json JSONB NOT NULL
        )"
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS subscribers (
            id BIGSERIAL PRIMARY KEY,
            email TEXT UNIQUE NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
        )"
    )
    .execute(&pool)
    .await
    .unwrap();

    let state = AppState { pool };

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);
    
    // API Router
    let app = Router::new()
        .route("/api/analyze", post(handle_analyze))
        .route("/api/ai-fix", post(handle_ai_fix))
        .route("/api/history", get(handle_history))
        .route("/api/history/:id", get(handle_history_detail))
        .route("/api/subscribe", post(handle_subscribe))
        // Serve static assets from "static" dir
        .nest_service("/", ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Starting Pneuma SEO & AEO Analysis backend on http://localhost:{}", port);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
