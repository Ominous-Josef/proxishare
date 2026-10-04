// All date and time formatting for the UI lives here, so wording and
// thresholds only ever need changing in one place.
import { differenceInDays, format, formatDistanceToNowStrict, fromUnixTime } from "date-fns";

/** Seconds since a unix timestamp (in seconds). */
const secondsSince = (timestamp: number): number => Math.floor(Date.now() / 1000 - timestamp);

/**
 * Human-friendly "last seen" for a device, from a unix timestamp in seconds:
 * "Online", "Just now", then "5 minutes ago", "3 days ago", "2 months ago"...
 */
export const formatLastSeen = (timestamp: number): string => {
  const seconds = secondsSince(timestamp);
  if (seconds < 10) return "Online";
  if (seconds < 60) return "Just now";
  return formatDistanceToNowStrict(fromUnixTime(timestamp), { addSuffix: true });
};

/** Whether a device was seen in the last two minutes (treated as active). */
export const isRecentlySeen = (timestamp: number): boolean => secondsSince(timestamp) < 120;

/**
 * Date of a transfer, from a unix timestamp in seconds: relative for the last
 * few days ("2 hours ago"), otherwise the date itself ("Aug 8, 2026").
 */
export const formatTransferDate = (timestamp: number): string => {
  const date = fromUnixTime(timestamp);
  if (differenceInDays(new Date(), date) > 3) {
    return format(date, "PP");
  }
  return formatDistanceToNowStrict(date, { addSuffix: true });
};

/** Estimated time left for a transfer: "45s", "3m 20s", "1h 5m", or "--" if unknown. */
export const formatTimeRemaining = (seconds: number): string => {
  if (!isFinite(seconds) || seconds <= 0) return "--";
  const total = Math.ceil(seconds);
  if (total < 60) return `${total}s`;
  if (total < 3600) return `${Math.floor(total / 60)}m ${total % 60}s`;
  return `${Math.floor(total / 3600)}h ${Math.floor((total % 3600) / 60)}m`;
};

/** Countdown clock: "4:59". */
export const formatCountdown = (seconds: number): string => {
  const total = Math.max(0, Math.floor(seconds));
  return `${Math.floor(total / 60)}:${(total % 60).toString().padStart(2, "0")}`;
};
