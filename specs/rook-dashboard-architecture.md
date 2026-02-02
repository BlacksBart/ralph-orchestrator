# Rook Dashboard Component Architecture

## Component Hierarchy

```
RookDashboard
├── Layout
│   ├── Header (status, connection, user)
│   ├── Sidebar (navigation, quick actions)
│   └── Main Content Area
│
├── Panels
│   ├── ContextPanel (current context visibility)
│   ├── SessionPanel (sub-agent management)
│   ├── MemoryPanel (memory browser)
│   ├── InteractionPanel (command interface)
│   └── LogPanel (system logs)
│
└── Shared Components
    ├── StatusBadge (running, idle, error, complete)
    ├── SearchInput (with debouncing)
    ├── CommandPalette (slash command interface)
    ├── Timeline (visual timeline of events)
    └── MemoryCard (individual memory display)
```

---

## Component Specifications

### Layout Components

#### Header
- **Props:** `connectionStatus`, `lastSyncTime`, `activeSessionCount`
- **Actions:** Settings toggle, manual refresh

#### Sidebar
- **Props:** `activePanel`, `quickActions`
- **Actions:** Panel navigation, quick action execution

#### Main Content Area
- **Props:** `activePanel`, `panelData`
- **Behavior:** Switches between panels based on navigation

---

### Panel Components

#### ContextPanel
```typescript
interface ContextPanelProps {
  currentContext: ContextSummary;
  recentConversations: Conversation[];
  activeTasks: Task[];
  memoryStats: MemoryStats;
}

interface ContextSummary {
  topics: string[];
  entities: Entity[];
  intent: string;
  confidence: number;
}
```

**Features:**
- Real-time context summary display
- Expandable conversation history
- Task status indicators with progress bars
- Memory utilization gauge

---

#### SessionPanel
```typescript
interface SessionPanelProps {
  sessions: Session[];
  onTerminate: (sessionId: string) => void;
  onViewDetails: (sessionId: string) => void;
}

interface Session {
  id: string;
  name: string;
  type: 'sub-agent' | 'task' | 'workflow';
  status: 'running' | 'idle' | 'error' | 'completed';
  startTime: Date;
  runtime: number; // seconds
  taskDescription: string;
  progress?: number; // 0-100
}
```

**Features:**
- Sortable/filterable session list
- Real-time status updates
- One-click terminate action
- Runtime timer (auto-updating)
- Error detail expansion

---

#### MemoryPanel
```typescript
interface MemoryPanelProps {
  memories: Memory[];
  categories: string[];
  onSearch: (query: string) => void;
  onFilter: (category: string) => void;
}

interface Memory {
  id: string;
  content: string;
  category: 'short-term' | 'long-term' | 'decision' | 'context';
  timestamp: Date;
  source: string;
  relevance: number; // 0-100
  tags: string[];
}
```

**Features:**
- Real-time search with highlighting
- Category filter pills
- Virtualized list for performance
- Expandable memory details
- Copy to clipboard action

---

#### InteractionPanel
```typescript
interface InteractionPanelProps {
  onSubmit: (input: string, context?: any) => void;
  suggestions: Suggestion[];
  history: string[];
  isProcessing: boolean;
}

interface Suggestion {
  command: string;
  description: string;
  icon?: string;
}
```

**Features:**
- Multi-line text input (textarea with auto-grow)
- Slash command autocomplete
- Command history (up/down navigation)
- Typing indicators
- Response streaming display
- Context attachment display

**Slash Commands:**
| Command | Description |
|---------|-------------|
| /task | Create a new task |
| /search | Search memories |
| /remember | Save something to memory |
| /status | Get system status |
| /clear | Clear conversation |
| /help | Show available commands |

---

#### LogPanel
```typescript
interface LogPanelProps {
  logs: LogEntry[];
  filters: LogFilter;
  autoScroll: boolean;
}

interface LogEntry {
  timestamp: Date;
  level: 'debug' | 'info' | 'warn' | 'error';
  source: string;
  message: string;
  metadata?: any;
}
```

**Features:**
- Color-coded log levels
- Source filtering
- Timestamp display
- Auto-scroll toggle
- Export functionality

---

### Shared Components

#### StatusBadge
```typescript
interface StatusBadgeProps {
  status: 'running' | 'idle' | 'error' | 'completed' | 'pending';
  showPulse?: boolean;
  size?: 'sm' | 'md' | 'lg';
}
```

**Visual States:**
- Running: Blue with pulse animation
- Idle: Gray static
- Error: Red with icon
- Completed: Green with checkmark
- Pending: Yellow with spinner

---

#### SearchInput
```typescript
interface SearchInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  debounceMs?: number;
  showClear?: boolean;
}
```

**Features:**
- Debounced input handling
- Clear button on non-empty
- Keyboard shortcut (Cmd/Ctrl+K focus)

---

#### CommandPalette
```typescript
interface CommandPaletteProps {
  commands: Command[];
  isOpen: boolean;
  onClose: () => void;
  onExecute: (command: Command) => void;
}

interface Command {
  id: string;
  name: string;
  shortcut?: string;
  category: string;
  icon?: string;
  action: () => void;
}
```

**Features:**
- Fuzzy search through commands
- Keyboard navigation (arrow keys + enter)
- Category grouping
- Shortcut display

---

## State Management

### Global State (App Level)
```typescript
interface AppState {
  activePanel: PanelType;
  connectionStatus: 'connected' | 'disconnected' | 'connecting';
  lastSyncTime: Date;
  settings: UserSettings;
}
```

### Panel-Specific State
Each panel manages its own data with server sync:
- ContextPanel: Polls `/api/context` every 5s
- SessionPanel: WebSocket or SSE for real-time updates
- MemoryPanel: Loads on demand, search is client-side
- InteractionPanel: Local state, submits to `/api/interact`

### Data Flow
```
User Action → Local State Update → API Call → Server Response → Global State Update → UI Refresh
```

---

## API Interface (Expected)

```typescript
// Context
GET /api/context → ContextSummary
GET /api/conversations?limit=5 → Conversation[]

// Sessions
GET /api/sessions → Session[]
DELETE /api/sessions/:id → void

// Memory
GET /api/memories?search=&category= → Memory[]
POST /api/memories → Memory

// Interaction
POST /api/interact → StreamingResponse
GET /api/suggestions?partial= → Suggestion[]

// Logs
GET /api/logs?level=&source= → LogEntry[]
```

---

## Styling Architecture

### CSS Variables
```css
:root {
  /* Colors */
  --bg-primary: #0f172a;
  --bg-secondary: #1e293b;
  --bg-tertiary: #334155;
  --text-primary: #f8fafc;
  --text-secondary: #94a3b8;
  --text-muted: #64748b;
  
  /* Status Colors */
  --status-running: #3b82f6;
  --status-idle: #64748b;
  --status-error: #ef4444;
  --status-success: #22c55e;
  --status-pending: #eab308;
  
  /* Spacing */
  --sidebar-width: 240px;
  --header-height: 48px;
  --panel-gap: 16px;
}
```

### Component Classes
- Use BEM naming: `.panel`, `.panel__header`, `.panel--active`
- Utility classes for common patterns
- Dark mode default with CSS variables
