import type { CaptureRecord, ExtractionRecord } from '$lib/api';

export interface TimelineCapture {
  capture: CaptureRecord;
  extraction: ExtractionRecord | null;
}

export interface TimelineHourBucket {
  key: string;
  heading: string;
  rangeLabel: string;
  captures: TimelineCapture[];
}
