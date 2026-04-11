export interface InsightEnvelope {
  id: number;
  insight_type: string;
  data: Record<string, unknown>;
  narrative?: string | null;
}

export interface RollingAppUsageView {
  name: string;
  share: string;
}

export interface RollingContextView {
  currentFocus: string;
  activeProject: string | null;
  summary: string | null;
  mood: string | null;
  appsUsed: RollingAppUsageView[];
}

export interface ProjectBreakdownView {
  name: string;
  minutes: number;
  durationLabel: string;
  accomplishments: string[];
}

export interface FocusBlockView {
  project: string;
  minutes: number;
  label: string;
  quality: string;
  tint: string;
}

export interface DailySummaryView {
  totalMinutes: number;
  totalLabel: string;
  focusScore: number | null;
  focusScoreLabel: string;
  narrative: string | null;
  keyMoments: string[];
  openThreads: string[];
  projects: ProjectBreakdownView[];
  focusBlocks: FocusBlockView[];
}
