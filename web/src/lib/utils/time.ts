import { format } from 'date-fns';

export function formatConsoleTime(date: Date): string {
  return format(date, "EEE · MMM d · HH:mm");
}
