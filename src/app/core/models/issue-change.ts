export interface ChangelogItem {
  fieldId: string | null;
  field: string;
  fieldType: string;
  fromId: string | null;
  fromString: string | null;
  toId: string | null;
  toString: string | null;
}

export interface IssueChange {
  changelogId: string;
  authorAccountId: string | null;
  authorDisplayName: string | null;
  jiraCreatedAt: string;
  items: ChangelogItem[];
}

export interface IssueChangelogResponse {
  earliestDate: string | null;
  latestDate: string | null;
  entries: IssueChange[];
}
