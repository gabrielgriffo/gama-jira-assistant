const MONTHS = ["JAN", "FEV", "MAR", "ABR", "MAI", "JUN", "JUL", "AGO", "SET", "OUT", "NOV", "DEZ"];

export interface MonthGroup {
  key: string;
  label: string;
  span: number;
}

export function monthGroups(dates: string[]): MonthGroup[] {
  const groups: MonthGroup[] = [];
  for (const date of dates) {
    const key = date.slice(0, 7);
    const last = groups[groups.length - 1];
    if (last?.key === key) {
      last.span++;
    } else {
      groups.push({ key, label: MONTHS[Number(date.slice(5, 7)) - 1], span: 1 });
    }
  }
  return groups;
}
