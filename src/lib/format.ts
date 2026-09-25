// FilePath: src/lib/format.ts
// Date and number formatting shared by the pages. Everything is local time: history rows,
// day groups and the insights heatmap all use the user's calendar, like the backend does.

const numberFormat = new Intl.NumberFormat();
const compactFormat = new Intl.NumberFormat(undefined, {
    notation: "compact",
    maximumFractionDigits: 1,
});
const timeFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const weekdayFormat = new Intl.DateTimeFormat(undefined, { weekday: "short" });
const dayMonthFormat = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const longDateFormat = new Intl.DateTimeFormat(undefined, {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
});
const monthFormat = new Intl.DateTimeFormat(undefined, { month: "short" });

export function formatNumber(value: number): string {
    return numberFormat.format(value);
}

/** 12.3K style for tight spaces; plain digits below ten thousand. */
export function formatCompact(value: number): string {
    return value < 10_000 ? numberFormat.format(value) : compactFormat.format(value);
}

export function pluralize(count: number, singular: string, plural = `${singular}s`): string {
    return count === 1 ? singular : plural;
}

export function formatTime(ms: number): string {
    return timeFormat.format(new Date(ms));
}

function pad(value: number): string {
    return String(value).padStart(2, "0");
}

/** Local calendar date as `YYYY-MM-DD`, the same shape as `DayActivity.date`. */
export function dayKey(date: Date): string {
    return `${String(date.getFullYear())}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Parses `YYYY-MM-DD` as local midnight (Date parsing would treat it as UTC). */
export function parseDayKey(key: string): Date {
    const [year = "1970", month = "1", day = "1"] = key.split("-");
    return new Date(Number(year), Number(month) - 1, Number(day));
}

export function startOfDay(date: Date): Date {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function addDays(date: Date, days: number): Date {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

/** Monday of the week containing `date`. */
export function startOfWeek(date: Date): Date {
    const day = startOfDay(date);
    const offset = (day.getDay() + 6) % 7;
    return addDays(day, -offset);
}

/** "Today", "Yesterday", or "Mon 22 Sep" (with the year when it is not the current one). */
export function dayLabel(date: Date, now: Date = new Date()): string {
    const today = startOfDay(now);
    const day = startOfDay(date);
    const diff = Math.round((today.getTime() - day.getTime()) / 86_400_000);
    if (diff === 0) return "Today";
    if (diff === 1) return "Yesterday";
    const base = `${weekdayFormat.format(day)} ${dayMonthFormat.format(day)}`;
    return day.getFullYear() === today.getFullYear()
        ? base
        : `${base} ${String(day.getFullYear())}`;
}

export function formatLongDate(date: Date): string {
    return longDateFormat.format(date);
}

export function formatMonth(date: Date): string {
    return monthFormat.format(date);
}

/** Minutes as "45 sec", "12 min" or "3 hr 20 min". */
export function formatMinutes(minutes: number): string {
    if (minutes <= 0) return "0 min";
    if (minutes < 1) return `${String(Math.max(1, Math.round(minutes * 60)))} sec`;
    const rounded = Math.round(minutes);
    if (rounded < 60) return `${String(rounded)} min`;
    const hours = Math.floor(rounded / 60);
    const rest = rounded % 60;
    return rest === 0 ? `${String(hours)} hr` : `${String(hours)} hr ${String(rest)} min`;
}
