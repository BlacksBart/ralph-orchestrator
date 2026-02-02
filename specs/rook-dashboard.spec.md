# Rook Dashboard Frontend Specification

## Overview
A web-based dashboard for the AI Chief of Staff (Rook) that provides visibility into context, active sessions, memory, and improved interaction workflows beyond traditional chat interfaces.

---

## Target User
**Mikey** - Developer who wants technical visibility into AI operations with quick access to context and actions.

---

## Core Features & Acceptance Criteria

### 1. Context Visibility Panel

**Given** the user opens the dashboard  
**When** viewing the main interface  
**Then** they see:
- Current context summary (what Rook knows right now)
- Recent conversation snippets (last 5 exchanges)
- Active tasks with status indicators
- Memory utilization metrics

**Given** context has changed  
**When** the dashboard auto-refreshes (5s interval)  
**Then** the context panel updates without full page reload

### 2. Session Management View

**Given** sub-agents are running  
**When** viewing the sessions panel  
**Then** display:
- Active sub-agent name and type
- Current status (running, idle, error, completed)
- Runtime duration
- Associated task description
- Kill/terminate action button

**Given** a session errors  
**When** the error occurs  
**Then** the status updates to "error" with expandable error details

### 3. Memory Browser

**Given** the user opens the memory panel  
**When** viewing memories  
**Then** they see:
- Searchable memory list
- Category filters (short-term, long-term, decisions, context)
- Timestamp for each memory
- Relevance/importance indicator
- Source (conversation, file, task result)

**Given** the user enters a search term  
**When** searching  
**Then** results filter in real-time with highlighted matches

### 4. Interaction Interface

**Given** the user wants to interact with Rook  
**When** using the interaction panel  
**Then** they have:
- Command palette style input (VS Code inspired)
- Slash commands for common operations (/task, /remember, /search)
- Context-aware suggestions as they type
- Multi-line input support for complex requests
- History navigation (up/down arrows)

**Given** a complex workflow  
**When** the user initiates it  
**Then** Rook shows progress steps, not just a chat response

### 5. Quick Actions Panel

**Given** the dashboard is loaded  
**When** viewing the sidebar  
**Then** quick action buttons are visible:
- New Task
- Search Memory
- View Logs
- System Status
- Settings

**Given** a quick action is clicked  
**When** the action executes  
**Then** appropriate panel opens or action executes immediately

---

## Non-Functional Requirements

### Performance
- Initial load: < 2 seconds
- Memory list: Virtualized scrolling for 1000+ items
- Search response: < 100ms for local filtering

### Security
- No sensitive data logged to browser console
- Memory content sanitized before display

### UX
- Keyboard shortcuts for all major actions
- Dark mode by default (developer-friendly)
- Responsive layout (works on tablet for mobile monitoring)

---

## Out of Scope
- User authentication (assumes local/single-user)
- Real-time collaborative features
- Mobile phone optimization
- Voice input/output
- Plugin/extension system (future consideration)

---

## Visual Design Direction

**Style:** Clean, technical, dark-themed  
**Inspiration:** VS Code, GitHub, Linear, Raycast  
**Colors:** Slate/dark backgrounds, blue accents, semantic status colors  
**Typography:** Monospace for technical content, sans-serif for UI elements

---

## Technical Stack Recommendation

- **Framework:** Vanilla HTML/CSS/JS (for prototype) or React/Vue (for production)
- **Styling:** Tailwind CSS or custom CSS variables
- **State:** LocalStorage for preferences, Server-Sent Events for real-time updates
- **Icons:** Lucide or Heroicons
