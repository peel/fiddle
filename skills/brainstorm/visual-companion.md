# Visual Companion Guide

Browser-based companion for showing mockups, diagrams, and visual options during a brainstorm.

## When to Use

Decide per question, not per session: would the user understand this better by seeing it than by reading it?

The browser suits content that is itself visual — UI mockups and wireframes, architecture and data-flow diagrams, side-by-side comparisons of two layouts or two design directions, questions about look and feel or visual hierarchy, and spatial relationships such as state machines and entity diagrams.

The terminal suits content that is text or tabular — requirements and scope questions, conceptual A/B/C choices between approaches described in words, tradeoff lists, technical decisions like API design or data modeling, and clarifying questions whose answer is words rather than a visual preference.

A question *about* a UI topic is not automatically a visual question. "What kind of wizard do you want?" is conceptual, so use the terminal; "which of these wizard layouts feels right?" is visual, so use the browser.

## How It Works

The server watches a directory for HTML files and serves the newest one. You write HTML to `screen_dir`, the user sees it in their browser and can click to select options, and selections are recorded to `state_dir/events` for you to read on your next turn.

If your HTML file starts with `<!DOCTYPE` or `<html>` the server serves it as-is and only injects the helper script; otherwise it wraps your content in the frame template, adding the header, CSS theme, selection indicator, and interactive infrastructure. Write content fragments by default, and full documents only when you need complete control over the page.

## Starting a Session

```bash
# Start server with persistence (mockups saved to project)
skills/brainstorm/scripts/start-server.sh --project-dir /path/to/project

# Returns: {"type":"server-started","port":52341,"url":"http://localhost:52341",
#           "screen_dir":"/path/to/project/.fiddle/brainstorm/12345-1706000000/content",
#           "state_dir":"/path/to/project/.fiddle/brainstorm/12345-1706000000/state"}
```

Save `screen_dir` and `state_dir` from the response and tell the user to open the URL.

Pass the project root as `--project-dir` so mockups persist in `.fiddle/brainstorm/` and survive server restarts; without it files go to `/tmp` and get cleaned up. Remind the user to add `.fiddle/` to `.gitignore` if it is not there already.

The server writes its startup JSON to `$STATE_DIR/server-info`. If you launched it in the background without capturing stdout, read that file for the URL and port; with `--project-dir`, the session directory is under `<project>/.fiddle/brainstorm/`.

The server has to keep running in the background across conversation turns, and harnesses differ in whether they allow that:

| Harness | Invocation |
|---|---|
| macOS / Linux | Default mode; the script backgrounds the server itself |
| Windows or foreground-only | Default mode (Windows auto-detects); launch via the harness's background command mechanism |
| Codex | Default mode; the script auto-detects `CODEX_CI` and switches to foreground |
| Gemini CLI | Add `--foreground` and set `is_background: true` on the shell tool call |
| Anything that reaps detached processes | Add `--foreground` and launch with the platform's background execution mechanism |

In foreground cases, read `$STATE_DIR/server-info` on the next turn to get the URL and port.

If the URL is unreachable from the browser, which is common in remote and containerized setups, bind a non-loopback host and control the printed hostname with `--url-host`:

```bash
skills/brainstorm/scripts/start-server.sh \
  --project-dir /path/to/project \
  --host 0.0.0.0 \
  --url-host localhost
```

## The Loop

1. **Check the server is alive, then write HTML** to a new file in `screen_dir`. Before each write, confirm `$STATE_DIR/server-info` exists; if it does not, or `$STATE_DIR/server-stopped` does, the server has shut down (it auto-exits after 30 minutes of inactivity) and needs restarting with `start-server.sh`. Use semantic filenames such as `platform.html`, `visual-style.html`, `layout.html`, give every screen a fresh file rather than reusing a name, and write with the harness's file-editing mechanism rather than cat or a heredoc, which dumps the markup into the terminal. The server serves the newest file by modification time.

2. **Tell the user what to expect and end your turn.** Repeat the URL every step, summarize what is on screen in a line ("showing 3 layout options for the homepage"), and ask them to respond in the terminal, clicking to select an option if they want to.

3. **On your next turn,** read `$STATE_DIR/events` if it exists for the browser interactions, and merge it with the user's terminal text. The terminal message is the primary feedback; the events file adds structured interaction data.

4. **Iterate or advance.** If the feedback changes the current screen, write a new file (`layout-v2.html`, then `layout-v3.html`). Move to the next question only once the current one is settled.

5. **Unload when returning to the terminal.** When the next step does not need the browser, push a waiting screen so the user is not staring at a resolved choice while the conversation has moved on:

   ```html
   <!-- filename: waiting.html (or waiting-2.html, etc.) -->
   <div style="display:flex;align-items:center;justify-content:center;min-height:60vh">
     <p class="subtitle">Continuing in terminal...</p>
   </div>
   ```

   Push a new content file as usual when the next visual question comes up.

6. Repeat until done.

## Writing Content Fragments

Write just the content that goes inside the page; the server supplies the frame, theme CSS, selection indicator, and scripts.

```html
<h2>Which layout works better?</h2>
<p class="subtitle">Consider readability and visual hierarchy</p>

<div class="options">
  <div class="option" data-choice="a" onclick="toggleSelect(this)">
    <div class="letter">A</div>
    <div class="content">
      <h3>Single Column</h3>
      <p>Clean, focused reading experience</p>
    </div>
  </div>
  <div class="option" data-choice="b" onclick="toggleSelect(this)">
    <div class="letter">B</div>
    <div class="content">
      <h3>Two Column</h3>
      <p>Sidebar navigation with main content</p>
    </div>
  </div>
</div>
```

No `<html>`, CSS, or `<script>` tags needed.

## CSS Classes Available

### Options (A/B/C choices)

```html
<div class="options">
  <div class="option" data-choice="a" onclick="toggleSelect(this)">
    <div class="letter">A</div>
    <div class="content">
      <h3>Title</h3>
      <p>Description</p>
    </div>
  </div>
</div>
```

Add `data-multiselect` to the container to let the user select several options; each click toggles an item and the indicator bar shows the count.

```html
<div class="options" data-multiselect>
  <!-- same option markup; users can select/deselect multiple -->
</div>
```

### Cards (visual designs)

```html
<div class="cards">
  <div class="card" data-choice="design1" onclick="toggleSelect(this)">
    <div class="card-image"><!-- mockup content --></div>
    <div class="card-body">
      <h3>Name</h3>
      <p>Description</p>
    </div>
  </div>
</div>
```

### Mockup container

```html
<div class="mockup">
  <div class="mockup-header">Preview: Dashboard Layout</div>
  <div class="mockup-body"><!-- your mockup HTML --></div>
</div>
```

### Split view (side-by-side)

```html
<div class="split">
  <div class="mockup"><!-- left --></div>
  <div class="mockup"><!-- right --></div>
</div>
```

### Pros/Cons

```html
<div class="pros-cons">
  <div class="pros"><h4>Pros</h4><ul><li>Benefit</li></ul></div>
  <div class="cons"><h4>Cons</h4><ul><li>Drawback</li></ul></div>
</div>
```

### Mock elements (wireframe building blocks)

```html
<div class="mock-nav">Logo | Home | About | Contact</div>
<div style="display: flex;">
  <div class="mock-sidebar">Navigation</div>
  <div class="mock-content">Main content area</div>
</div>
<button class="mock-button">Action Button</button>
<input class="mock-input" placeholder="Input field">
<div class="placeholder">Placeholder area</div>
```

### Typography and sections

- `h2` — page title
- `h3` — section heading
- `.subtitle` — secondary text below title
- `.section` — content block with bottom margin
- `.label` — small uppercase label text

## Browser Events Format

Clicks are recorded to `$STATE_DIR/events`, one JSON object per line, and the file is cleared when you push a new screen.

```jsonl
{"type":"click","choice":"a","text":"Option A - Simple Layout","timestamp":1706000101}
{"type":"click","choice":"c","text":"Option C - Complex Grid","timestamp":1706000108}
{"type":"click","choice":"b","text":"Option B - Hybrid","timestamp":1706000115}
```

The full stream shows the exploration path, since users often click several options before settling. The last `choice` event is typically the final selection, but the click pattern can reveal hesitation worth asking about. If the file does not exist, the user did not interact with the browser, so work from their terminal text alone.

## Design Tips

Scale fidelity to the question: wireframes for layout questions, polish for polish questions. State the question on the page itself ("which layout feels more professional?") rather than just "pick one". Keep to two to four options per screen, and keep mockups focused on layout and structure rather than pixel-perfect design. Use real content where it changes the judgment: a photography portfolio needs actual images, because placeholder content hides the design problems.

## Cleaning Up

```bash
skills/brainstorm/scripts/stop-server.sh $SESSION_DIR
```

Sessions started with `--project-dir` keep their mockups in `.fiddle/brainstorm/` for later reference; only `/tmp` sessions are deleted on stop.

## Reference

- Frame template (CSS reference): `scripts/frame-template.html`
- Helper script (client-side): `scripts/helper.js`
