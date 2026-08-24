# Pneuma SEO & AEO Analyzer

A fast, Rust-powered web crawler and auditing tool that evaluates websites for both traditional **Search Engine Optimization (SEO)** and modern **Answer Engine Optimization (AEO)**—optimizing your content to be parsed and cited by AI search engines, LLMs, and conversational agents (like ChatGPT, Claude, Perplexity, and Gemini).

---

## 🚀 Key Features

*   **Custom Depth Crawler**: Recursively crawls web pages starting from a seed URL with user-defined depth and page limits.
*   **Dual Audit Engine**:
    *   **SEO Audit**: Title tags, Meta descriptions, H1 density, Image alt texts, Canonical URLs, and Mobile responsiveness (Viewport).
    *   **AEO Audit**: Robots.txt verification for AI/LLM crawlers, JSON-LD structured data validation, text extractability scores, and E-E-A-T (Experience, Expertise, Authoritativeness, Trustworthiness) credentials and citations.
*   **AI-Assisted Fixes**: Integrates with the **DeepSeek API** to provide instant, actionable code recommendations and fixes for failed rules.
*   **Audit History**: Persists past crawls and reports to a **PostgreSQL** database for performance tracking over time.
*   **Clean Dashboard**: Interactive, responsive UI showing detailed per-page breakdown scores, issues list, and recommendation panels.

---

## 🛠️ Tech Stack

*   **Backend**: [Rust](https://www.rust-lang.org/) & [Axum](https://github.com/tokio-rs/axum) (Web Framework), [Tokio](https://tokio.rs/) (Async Runtime), [SQLx](https://github.com/launchbadge/sqlx) (PostgreSQL client).
*   **Frontend**: Vanilla HTML5, CSS3 (Tailwind-like custom theme), and asynchronous JavaScript.
*   **AI Integration**: [DeepSeek API](https://www.deepseek.com/) for automated solutions.

---

## 💻 Local Setup

### Prerequisites
*   Rust toolchain (2024 edition or newer)
*   PostgreSQL running locally

### 1. Environment Variables
Create a `.env` file in the root directory:
```env
DATABASE_URL=postgres://username:password@localhost:5432/seo_aeo
DEEPSEEK_API_KEY=your_deepseek_api_key
PORT=8080
```

### 2. Run the Server
The application automatically creates the necessary tables on startup:
```bash
cargo run
```
Once started, the backend API will run on `http://localhost:8080` and serve the frontend directly.

---

## 🌐 Cloud Deployment

This project is configured to run smoothly in a decoupled environment:
*   **Backend & DB on Railway**: Automatically built via Nixpacks. Uses a PostgreSQL service database.
*   **Frontend on Vercel**: Serves the `static/` folder. Employs Vercel's edge rewrites in `vercel.json` to proxy `/api/*` requests to the Railway service.

---

## 🤝 Contributing

Open-source contributions are highly welcomed! Whether you want to fix a bug, add a new SEO or AEO audit rule, improve the crawler's performance, or enhance the dashboard UI, feel free to contribute:
1. Fork the repository.
2. Create your feature branch (`git checkout -b feature/AmazingFeature`).
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`).
4. Push to the branch (`git push origin feature/AmazingFeature`).
5. Open a Pull Request.
