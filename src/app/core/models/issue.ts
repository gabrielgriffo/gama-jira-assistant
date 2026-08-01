export interface Issue {
  id: string;
  key: string;
  summary: string;
  statusId: string;
  statusName: string;
  statusCategoryKey: string;
  assigneeAccountId: string | null;
  assigneeDisplayName: string | null;
  priorityName: string | null;
  issueTypeName: string | null;
  dueDate: string | null;
  browseUrl: string;
  jiraUpdatedAt: string;
}
