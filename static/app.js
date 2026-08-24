document.addEventListener('DOMContentLoaded', () => {
    const form = document.getElementById('analyze-form');
    const submitBtn = document.getElementById('submit-btn');
    const btnLoader = submitBtn.querySelector('.btn-loader');
    const btnText = submitBtn.querySelector('.btn-text');
    const btnIcon = submitBtn.querySelector('.btn-icon');
    const loadingContainer = document.getElementById('loading-container');
    const errorContainer = document.getElementById('error-container');
    const errorMessage = document.getElementById('error-message');
    const resultsContainer = document.getElementById('results-container');
    
    // Gauges
    const seoScoreVal = document.getElementById('seo-score-value');
    const seoGaugeFill = document.getElementById('seo-gauge-fill');
    const aeoScoreVal = document.getElementById('aeo-score-value');
    const aeoGaugeFill = document.getElementById('aeo-gauge-fill');
    
    // Lists
    const topIssuesList = document.getElementById('top-issues-list');
    const pagesListContainer = document.getElementById('pages-list-container');

    // Subscription Modal elements
    const authModal = document.getElementById('auth-modal');
    const subscribeForm = document.getElementById('subscribe-form');
    const subscriberEmailInput = document.getElementById('subscriber-email');
    const subscribeError = document.getElementById('subscribe-error');
    const closeModalBtn = document.getElementById('close-modal-btn');
    let pendingSubmission = null;

    // =========================================================================
    // Theme Toggle
    // =========================================================================
    const themeToggle = document.getElementById('theme-toggle');
    const htmlEl = document.documentElement;
    
    // Load saved theme or default to light
    const savedTheme = localStorage.getItem('pneuma-theme');
    if (savedTheme) {
        htmlEl.setAttribute('data-theme', savedTheme);
    } else {
        htmlEl.setAttribute('data-theme', 'light');
    }

    themeToggle.addEventListener('click', () => {
        const current = htmlEl.getAttribute('data-theme');
        const next = current === 'dark' ? 'light' : 'dark';
        htmlEl.setAttribute('data-theme', next);
        localStorage.setItem('pneuma-theme', next);
    });

    // =========================================================================
    // Form Submit
    // =========================================================================
    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        
        const urlInput = document.getElementById('site-url').value.trim();
        const maxDepth = parseInt(document.getElementById('max-depth').value, 10);
        const maxPages = parseInt(document.getElementById('max-pages').value, 10);

        if (!urlInput) return;

        // Check if subscribed
        const isSubscribed = localStorage.getItem('pneuma-subscribed');
        if (!isSubscribed) {
            pendingSubmission = { urlInput, maxDepth, maxPages };
            authModal.classList.remove('hidden');
            return;
        }

        runAnalysis(urlInput, maxDepth, maxPages);
    });

    async function runAnalysis(urlInput, maxDepth, maxPages) {
        // Reset UI States
        errorContainer.classList.add('hidden');
        resultsContainer.classList.add('hidden');
        loadingContainer.classList.remove('hidden');
        submitBtn.disabled = true;
        btnLoader.classList.remove('hidden');
        btnText.textContent = 'Analyzing…';
        if (btnIcon) btnIcon.style.display = 'none';

        try {
            const res = await fetch('/api/analyze', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    url: urlInput,
                    max_depth: maxDepth,
                    max_pages: maxPages
                })
            });

            if (!res.ok) {
                const errData = await res.json();
                throw new Error(errData.error || `HTTP error! status: ${res.status}`);
            }

            const report = await res.json();
            renderDashboard(report);
            
        } catch (error) {
            console.error('Analysis error:', error);
            errorMessage.textContent = error.message;
            errorContainer.classList.remove('hidden');
        } finally {
            loadingContainer.classList.add('hidden');
            submitBtn.disabled = false;
            btnLoader.classList.add('hidden');
            btnText.textContent = 'Analyze';
            if (btnIcon) btnIcon.style.display = '';
        }
    }

    // Modal Events
    subscribeForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const email = subscriberEmailInput.value.trim();
        if (email) {
            const submitBtn = subscribeForm.querySelector('button[type="submit"]');
            const originalText = submitBtn.textContent;
            submitBtn.disabled = true;
            submitBtn.textContent = 'Unlocking…';
            subscribeError.classList.add('hidden');

            try {
                const res = await fetch('/api/subscribe', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ email })
                });

                if (!res.ok) {
                    const errData = await res.json();
                    throw new Error(errData.error || `Server error: ${res.status}`);
                }

                localStorage.setItem('pneuma-subscribed', 'true');
                localStorage.setItem('pneuma-email', email);
                authModal.classList.add('hidden');
                
                if (pendingSubmission) {
                    runAnalysis(pendingSubmission.urlInput, pendingSubmission.maxDepth, pendingSubmission.maxPages);
                    pendingSubmission = null;
                }
            } catch (err) {
                console.error('Subscription sync error:', err);
                subscribeError.textContent = err.message;
                subscribeError.classList.remove('hidden');
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = originalText;
            }
        }
    });

    closeModalBtn.addEventListener('click', () => {
        authModal.classList.add('hidden');
        subscribeError.classList.add('hidden');
        pendingSubmission = null;
    });

    authModal.addEventListener('click', (e) => {
        if (e.target === authModal) {
            authModal.classList.add('hidden');
            subscribeError.classList.add('hidden');
            pendingSubmission = null;
        }
    });

    // =========================================================================
    // Render Dashboard
    // =========================================================================
    function renderDashboard(report) {
        // 1. Render Gauges
        animateGauge(seoGaugeFill, seoScoreVal, report.site_seo_score, 'seo');
        animateGauge(aeoGaugeFill, aeoScoreVal, report.site_aeo_score, 'aeo');

        // 2. Render Top Issues
        topIssuesList.innerHTML = '';
        if (report.top_issues.length === 0) {
            topIssuesList.innerHTML = `
                <div class="all-passed-card">
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/>
                        <polyline points="22 4 12 14.01 9 11.01"/>
                    </svg>
                    <span>All rules passed successfully across all crawled pages.</span>
                </div>
            `;
        } else {
            report.top_issues.forEach((issue, idx) => {
                const div = document.createElement('div');
                div.className = 'issue-item';
                
                let sevClass = 'severity-info';
                let dotColor = 'var(--text-muted)';
                if (issue.severity === 'Critical') { sevClass = 'severity-critical'; dotColor = 'var(--accent-rose)'; }
                if (issue.severity === 'Warning') { sevClass = 'severity-warning'; dotColor = 'var(--accent-amber)'; }

                const failRate = issue.total_count > 0 ? Math.round((issue.failure_count / issue.total_count) * 100) : 0;
                const fixBtnId = `ai-fix-btn-top-${idx}`;
                const fixContainerId = `ai-fix-result-top-${idx}`;

                div.innerHTML = `
                    <div class="issue-main-content">
                        <div class="issue-indicator">
                            <span class="issue-dot" style="background:${dotColor}"></span>
                            <span class="issue-severity ${sevClass}">${issue.severity}</span>
                        </div>
                        <div class="issue-details">
                            <h4>${escapeHtml(issue.name)}</h4>
                            <p>Failed on ${issue.failure_count} of ${issue.total_count} pages</p>
                        </div>
                        <div class="issue-frequency">
                            <div class="freq-bar-track">
                                <div class="freq-bar-fill" style="width:${failRate}%; background:${dotColor}"></div>
                            </div>
                            <span class="freq-label">${failRate}% affected</span>
                        </div>
                    </div>
                    <div class="issue-ai-fix-section">
                        <button class="ai-fix-btn" id="${fixBtnId}" 
                            data-rule-id="${escapeHtml(issue.rule_id || '')}"
                            data-rule-name="${escapeHtml(issue.name)}"
                            data-rule-message="This rule failed across multiple pages. Please provide a general fix or optimization."
                            data-page-url="Multiple Pages"
                            data-category="${issue.category}">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 2a4 4 0 0 1 4 4c0 1.95-1.4 3.58-3.25 3.93L12 22"/><path d="M9 6.5C7.14 7.5 6 9.5 6 12c0 3.5 2.5 6 6 6s6-2.5 6-6c0-2.5-1.14-4.5-3-5.5"/></svg>
                            <span>AI Fix</span>
                        </button>
                        <div class="ai-fix-result hidden" id="${fixContainerId}"></div>
                    </div>
                `;
                topIssuesList.appendChild(div);
            });
        }

        // 3. Render Pages list
        pagesListContainer.innerHTML = '';
        report.per_page_reports.forEach((page, index) => {
            const pageDiv = document.createElement('div');
            pageDiv.className = 'page-row';
            
            const isSuccess = page.status >= 200 && page.status < 300;
            const statusClass = isSuccess ? 'meta-status' : 'meta-status err';
            
            // Build checklists
            let seoRulesHtml = '';
            let aeoRulesHtml = '';
            
            page.outcomes.forEach((o, ruleIdx) => {
                const iconSvg = o.passed
                    ? `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><polyline points="20 6 9 17 4 12"/></svg>`
                    : `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>`;
                const passClass = o.passed ? 'check-pass' : 'check-fail';
                const fixBtnId = `ai-fix-btn-${index}-${ruleIdx}`;
                const fixContainerId = `ai-fix-result-${index}-${ruleIdx}`;
                const aiFix = !o.passed ? `
                    <button class="ai-fix-btn" id="${fixBtnId}" 
                        data-rule-id="${escapeHtml(o.rule_id || '')}"
                        data-rule-name="${escapeHtml(o.name)}"
                        data-rule-message="${escapeHtml(o.message || o.description)}"
                        data-page-url="${escapeHtml(page.url)}"
                        data-category="${o.category}">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 2a4 4 0 0 1 4 4c0 1.95-1.4 3.58-3.25 3.93L12 22"/><path d="M9 6.5C7.14 7.5 6 9.5 6 12c0 3.5 2.5 6 6 6s6-2.5 6-6c0-2.5-1.14-4.5-3-5.5"/></svg>
                        <span>AI Fix</span>
                    </button>
                    <div class="ai-fix-result hidden" id="${fixContainerId}"></div>
                ` : '';
                const ruleHtml = `
                    <div class="checklist-item">
                        <span class="check-icon ${passClass}">${iconSvg}</span>
                        <div class="rule-text">
                            <span class="rule-name">${escapeHtml(o.name)}</span>
                            <span class="rule-msg">${escapeHtml(o.message || o.description)}</span>
                            ${aiFix}
                        </div>
                    </div>
                `;
                if (o.category === 'SEO') {
                    seoRulesHtml += ruleHtml;
                } else {
                    aeoRulesHtml += ruleHtml;
                }
            });

            // Build AEO section analysis list
            let aeoSectionsHtml = '';
            if (page.extractability_sections.length === 0) {
                aeoSectionsHtml = '<p class="no-sections-msg">No H2/H3 header sections found on this page.</p>';
            } else {
                page.extractability_sections.forEach(sec => {
                    let scoreClass = 'score-low';
                    if (sec.score >= 80) scoreClass = 'score-high';
                    else if (sec.score >= 50) scoreClass = 'score-mid';
                    
                    const detailsList = sec.details.map(d => `<li>${escapeHtml(d)}</li>`).join('');
                    
                    aeoSectionsHtml += `
                        <div class="aeo-section-card">
                            <div class="aeo-section-header">
                                <h5>${escapeHtml(sec.heading)}</h5>
                                <span class="aeo-section-score ${scoreClass}">${sec.score}/100</span>
                            </div>
                            <div class="aeo-score-bar-track">
                                <div class="aeo-score-bar-fill ${scoreClass}" style="width:${sec.score}%"></div>
                            </div>
                            <div class="aeo-section-body">${escapeHtml(sec.text.substring(0, 160))}…</div>
                            <ul class="aeo-section-details">
                                ${detailsList}
                            </ul>
                        </div>
                    `;
                });
            }

            // Mini score bars for page summary
            const seoWidth = Math.round(page.seo_score);
            const aeoWidth = Math.round(page.aeo_score);

            pageDiv.innerHTML = `
                <div class="page-summary" onclick="togglePageRow(this.parentNode)">
                    <div class="page-info">
                        <div class="page-url">${escapeHtml(page.url)}</div>
                        <div class="page-meta">
                            <span class="${statusClass}">HTTP ${page.status}</span>
                            <span class="meta-sep">·</span>
                            <span>${page.load_time_ms}ms</span>
                        </div>
                    </div>
                    <div class="page-scores">
                        <div class="p-score seo">
                            <span class="p-score-label">SEO</span>
                            <div class="p-score-bar-track"><div class="p-score-bar-fill" style="width:${seoWidth}%"></div></div>
                            <span class="p-score-value">${seoWidth}%</span>
                        </div>
                        <div class="p-score aeo">
                            <span class="p-score-label">AEO</span>
                            <div class="p-score-bar-track"><div class="p-score-bar-fill" style="width:${aeoWidth}%"></div></div>
                            <span class="p-score-value">${aeoWidth}%</span>
                        </div>
                        <span class="chevron">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><polyline points="6 9 12 15 18 9"/></svg>
                        </span>
                    </div>
                </div>
                <div class="page-details">
                    <div class="details-grid">
                        <div class="details-column">
                            <h4>SEO Compliance</h4>
                            <div class="rule-checklist">${seoRulesHtml}</div>
                        </div>
                        <div class="details-column">
                            <h4>AEO Compliance</h4>
                            <div class="rule-checklist">${aeoRulesHtml}</div>
                        </div>
                    </div>
                    <div class="details-column extractability-column">
                        <h4>AEO Section Extractability</h4>
                        <div class="aeo-sections-list">${aeoSectionsHtml}</div>
                    </div>
                </div>
            `;
            pagesListContainer.appendChild(pageDiv);
        });

        // Show Results container
        resultsContainer.classList.remove('hidden');

        // Smooth scroll to results
        setTimeout(() => {
            resultsContainer.scrollIntoView({ behavior: 'smooth', block: 'start' });
        }, 100);
    }

    // =========================================================================
    // Gauge Animation
    // =========================================================================
    function animateGauge(circleEl, textEl, targetScore, type) {
        const score = Math.round(targetScore);
        const radius = circleEl.r.baseVal.value;
        const circumference = 2 * Math.PI * radius;
        
        circleEl.style.strokeDasharray = circumference;
        circleEl.style.strokeDashoffset = circumference;

        // Color the gauge based on score
        let color;
        if (score >= 80) color = 'var(--accent-emerald)';
        else if (score >= 50) color = 'var(--accent-amber)';
        else color = 'var(--accent-rose)';

        // For the type-specific default when score is good
        if (score >= 80) {
            color = type === 'seo' ? 'var(--accent-blue)' : 'var(--accent-emerald)';
        }
        circleEl.style.stroke = color;

        // Color the score text
        textEl.style.color = color;
        
        let currentScore = 0;
        const duration = 1000;
        const steps = 60;
        const stepTime = duration / steps;
        
        const interval = setInterval(() => {
            currentScore += score / steps;
            if (currentScore >= score) {
                currentScore = score;
                clearInterval(interval);
            }
            
            textEl.textContent = `${Math.round(currentScore)}%`;
            const offset = circumference - (currentScore / 100) * circumference;
            circleEl.style.strokeDashoffset = offset;
        }, stepTime);
    }

    // =========================================================================
    // Utilities
    // =========================================================================
    function escapeHtml(str) {
        if (!str) return '';
        return str
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#039;');
    }

    // =========================================================================
    // AI Fix Handler (Event Delegation)
    // =========================================================================
    document.addEventListener('click', async (e) => {
        const btn = e.target.closest('.ai-fix-btn');
        if (!btn) return;

        const resultId = btn.id.replace('ai-fix-btn-', 'ai-fix-result-');
        const resultDiv = document.getElementById(resultId);
        if (!resultDiv) return;

        // If already showing, toggle off
        if (!resultDiv.classList.contains('hidden')) {
            resultDiv.classList.add('hidden');
            return;
        }

        // Set loading state
        btn.disabled = true;
        const originalText = btn.querySelector('span').textContent;
        btn.querySelector('span').textContent = 'Thinking…';
        resultDiv.innerHTML = '<div class="ai-fix-loading">Generating fix with AI…</div>';
        resultDiv.classList.remove('hidden');

        try {
            const res = await fetch('/api/ai-fix', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    rule_id: btn.dataset.ruleId,
                    rule_name: btn.dataset.ruleName,
                    rule_message: btn.dataset.ruleMessage,
                    page_url: btn.dataset.pageUrl,
                    category: btn.dataset.category,
                })
            });

            const data = await res.json();
            resultDiv.innerHTML = `
                <div class="ai-fix-content">
                    <div class="ai-fix-header">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 2a4 4 0 0 1 4 4c0 1.95-1.4 3.58-3.25 3.93L12 22"/><path d="M9 6.5C7.14 7.5 6 9.5 6 12c0 3.5 2.5 6 6 6s6-2.5 6-6c0-2.5-1.14-4.5-3-5.5"/></svg>
                        <span>AI Suggestion</span>
                    </div>
                    <div class="ai-fix-body">${simpleMarkdown(data.fix)}</div>
                </div>
            `;
        } catch (err) {
            resultDiv.innerHTML = `<div class="ai-fix-error">Failed to get AI fix: ${escapeHtml(err.message)}</div>`;
        } finally {
            btn.disabled = false;
            btn.querySelector('span').textContent = originalText;
        }
    });

    // Basic markdown to HTML
    function simpleMarkdown(text) {
        if (!text) return '';
        let html = escapeHtml(text);
        // Code blocks (```...```)
        html = html.replace(/```(\w*)\n?([\s\S]*?)```/g, '<pre><code>$2</code></pre>');
        // Inline code
        html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
        // Bold
        html = html.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
        // Italic
        html = html.replace(/\*(.+?)\*/g, '<em>$1</em>');
        // Headings
        html = html.replace(/^### (.+)$/gm, '<strong>$1</strong>');
        html = html.replace(/^## (.+)$/gm, '<strong>$1</strong>');
        html = html.replace(/^# (.+)$/gm, '<strong>$1</strong>');
        // Unordered list items
        html = html.replace(/^- (.+)$/gm, '• $1');
        // Line breaks
        html = html.replace(/\n/g, '<br>');
        return html;
    }

    // =========================================================================
    // History Drawer Logic
    // =========================================================================
    const historyToggle = document.getElementById('history-toggle');
    const historyDrawer = document.getElementById('history-drawer');
    const closeHistoryBtn = document.getElementById('close-history-btn');
    const historyList = document.getElementById('history-list');

    historyToggle.addEventListener('click', async () => {
        historyDrawer.classList.remove('hidden');
        await loadHistory();
    });

    closeHistoryBtn.addEventListener('click', () => {
        historyDrawer.classList.add('hidden');
    });

    historyDrawer.addEventListener('click', (e) => {
        if (e.target === historyDrawer) {
            historyDrawer.classList.add('hidden');
        }
    });

    async function loadHistory() {
        try {
            historyList.innerHTML = '<div class="drawer-empty">Loading history...</div>';
            const res = await fetch('/api/history');
            if (!res.ok) throw new Error('Failed to load history');
            const items = await res.json();
            
            if (items.length === 0) {
                historyList.innerHTML = '<div class="drawer-empty">No previous audits found.</div>';
                return;
            }

            historyList.innerHTML = '';
            items.forEach(item => {
                const card = document.createElement('div');
                card.className = 'history-card';
                card.addEventListener('click', () => loadHistoryDetail(item.id));

                const dateStr = formatDate(item.created_at);

                card.innerHTML = `
                    <div class="history-card-header">
                        <span class="history-card-url" title="${escapeHtml(item.url)}">${escapeHtml(item.url)}</span>
                        <span class="history-card-date">${dateStr}</span>
                    </div>
                    <div class="history-card-scores">
                        <div class="history-card-score seo">
                            <span>SEO</span>
                            <strong>${Math.round(item.seo_score)}%</strong>
                        </div>
                        <div class="history-card-score aeo">
                            <span>AEO</span>
                            <strong>${Math.round(item.aeo_score)}%</strong>
                        </div>
                    </div>
                `;
                historyList.appendChild(card);
            });
        } catch (err) {
            historyList.innerHTML = `<div class="drawer-empty" style="color:var(--accent-rose)">Error: ${escapeHtml(err.message)}</div>`;
        }
    }

    async function loadHistoryDetail(id) {
        try {
            historyDrawer.classList.add('hidden');
            errorContainer.classList.add('hidden');
            resultsContainer.classList.add('hidden');
            loadingContainer.classList.remove('hidden');

            const res = await fetch(`/api/history/${id}`);
            if (!res.ok) throw new Error('Failed to fetch historical report');
            
            const report = await res.json();
            renderDashboard(report);
        } catch (err) {
            errorMessage.textContent = err.message;
            errorContainer.classList.remove('hidden');
        } finally {
            loadingContainer.classList.add('hidden');
        }
    }

    function formatDate(isoStr) {
        try {
            const clean = isoStr.replace('T', ' ').split('.')[0];
            return clean;
        } catch (_) {
            return isoStr;
        }
    }
});

// =========================================================================
// Page Accordion Toggle (global for onclick)
// =========================================================================
function togglePageRow(rowEl) {
    const details = rowEl.querySelector('.page-details');
    const isOpen = rowEl.classList.toggle('open');
    if (isOpen) {
        details.classList.add('open');
    } else {
        details.classList.remove('open');
    }
}
