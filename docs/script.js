/* Rune Code — Minimal Scripts */
(function () {
    'use strict';

    /* ── Nav scroll ── */
    const nav = document.getElementById('nav');
    window.addEventListener('scroll', () => {
        nav.classList.toggle('scrolled', window.scrollY > 20);
    });

    /* ── Mobile toggle ── */
    const toggle = document.getElementById('navToggle');
    const links = document.getElementById('navLinks');
    if (toggle) {
        toggle.addEventListener('click', () => links.classList.toggle('open'));
        links.querySelectorAll('a').forEach(a => a.addEventListener('click', () => links.classList.remove('open')));
    }

    /* ── Smooth anchor scroll ── */
    document.querySelectorAll('a[href^="#"]').forEach(a => {
        a.addEventListener('click', e => {
            const t = document.querySelector(a.getAttribute('href'));
            if (t) { e.preventDefault(); window.scrollTo({ top: t.offsetTop - 64, behavior: 'smooth' }); }
        });
    });

    /* ── Fade-in on scroll ── */
    function initFade() {
        const els = document.querySelectorAll(
            '.section-header, .features, .crates, .philosophy, .roadmap, .start-grid, .strip, .hero-text, .hero-terminal'
        );
        els.forEach(el => el.classList.add('fade-in'));

        const obs = new IntersectionObserver(entries => {
            entries.forEach(e => { if (e.isIntersecting) { e.target.classList.add('visible'); obs.unobserve(e.target); } });
        }, { threshold: 0.15, rootMargin: '0px 0px -30px 0px' });

        els.forEach(el => obs.observe(el));
    }

    /* ── Terminal typing ── */
    const scenes = [
        {
            cmd: 'rune prompt "explain this codebase"',
            out: `<span class="t-dim">Connecting to claude-opus-4-6...</span>

<span class="t-grn">This is a Rust workspace with 9 crates implementing
an autonomous coding harness.</span> Key components:

  <span class="t-acc">api</span>        Anthropic API client + SSE streaming
  <span class="t-acc">runtime</span>    Conversation loop, config, sessions
  <span class="t-acc">tools</span>      16 built-in tool implementations
  <span class="t-acc">cli</span>        Interactive REPL + one-shot prompt

<span class="t-dim">Cost: $0.012 | 847 in / 234 out tokens</span>`
        },
        {
            cmd: 'rune --model sonnet status',
            out: `<span class="t-pur">Rune Status</span>
<span class="t-dim">──────────────────────────────────</span>
  Model        <span class="t-acc">claude-sonnet-4-6</span>
  Session      <span class="t-grn">active</span>  a8f3c2
  Permissions  workspace-write
  Tools        16 loaded
  MCP          2 servers connected
  Branch       main (clean)
<span class="t-dim">──────────────────────────────────</span>`
        },
        {
            cmd: 'rune --permission-mode read-only prompt "review Cargo.toml"',
            out: `<span class="t-dim">read-only | claude-opus-4-6</span>

<span class="t-grn">Cargo.toml defines 9 workspace members.</span>

  <span class="t-acc">1</span> Workspace-level [workspace.dependencies]
  <span class="t-acc">2</span> All crates use edition = "2021"
  <span class="t-acc">3</span> Core stack: tokio, reqwest, serde
  <span class="t-acc">4</span> Zero unsafe blocks across all crates

<span class="t-dim">Cost: $0.008 | 612 in / 189 out tokens</span>`
        }
    ];

    let scene = 0, timer;

    function typeScene() {
        const cmd = document.getElementById('termCmd');
        const out = document.getElementById('termOut');
        const caret = document.querySelector('.term-caret');
        if (!cmd || !out) return;

        cmd.textContent = '';
        out.innerHTML = '';
        if (caret) caret.style.display = '';
        let i = 0;
        const s = scenes[scene];

        (function tick() {
            if (i < s.cmd.length) {
                cmd.textContent += s.cmd[i++];
                timer = setTimeout(tick, 28 + Math.random() * 40);
            } else {
                if (caret) caret.style.display = 'none';
                timer = setTimeout(() => {
                    out.innerHTML = s.out;
                    out.style.opacity = '0';
                    requestAnimationFrame(() => {
                        out.style.transition = 'opacity .35s';
                        out.style.opacity = '1';
                    });
                    timer = setTimeout(() => {
                        scene = (scene + 1) % scenes.length;
                        typeScene();
                    }, 3800);
                }, 350);
            }
        })();
    }

    function initTerminal() {
        const el = document.querySelector('.hero-terminal');
        if (!el) return;
        const obs = new IntersectionObserver(entries => {
            if (entries[0].isIntersecting) { typeScene(); obs.unobserve(el); }
        }, { threshold: 0.4 });
        obs.observe(el);
    }

    /* ── Boot ── */
    document.addEventListener('DOMContentLoaded', () => { initFade(); initTerminal(); });
})();