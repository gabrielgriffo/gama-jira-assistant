import arrowDownRight from "../../../../assets/icons/arrow-down-right.svg";
import arrowUpRight from "../../../../assets/icons/arrow-up-right.svg";
import brand from "../../../../assets/icons/brand.svg";
import calendar from "../../../../assets/icons/calendar.svg";
import check from "../../../../assets/icons/check.svg";
import chevronDown from "../../../../assets/icons/chevron-down.svg";
import chevronUp from "../../../../assets/icons/chevron-up.svg";
import clock from "../../../../assets/icons/clock.svg";
import close from "../../../../assets/icons/close.svg";
import download from "../../../../assets/icons/download.svg";
import gear from "../../../../assets/icons/gear.svg";
import history from "../../../../assets/icons/history.svg";
import list from "../../../../assets/icons/list.svg";
import search from "../../../../assets/icons/search.svg";
import send from "../../../../assets/icons/send.svg";
import sync from "../../../../assets/icons/sync.svg";
import windowClose from "../../../../assets/icons/window-close.svg";
import windowMaximize from "../../../../assets/icons/window-maximize.svg";
import windowMinimize from "../../../../assets/icons/window-minimize.svg";
import windowRestore from "../../../../assets/icons/window-restore.svg";
export const ICONS = {
  "arrow-down-right": arrowDownRight,
  "arrow-up-right": arrowUpRight,
  brand: brand,
  calendar: calendar,
  check: check,
  "chevron-down": chevronDown,
  "chevron-up": chevronUp,
  clock: clock,
  close: close,
  download: download,
  gear: gear,
  history: history,
  list: list,
  search: search,
  send: send,
  sync: sync,
  "window-close": windowClose,
  "window-maximize": windowMaximize,
  "window-minimize": windowMinimize,
  "window-restore": windowRestore,
} as const;
export type IconName = keyof typeof ICONS;
