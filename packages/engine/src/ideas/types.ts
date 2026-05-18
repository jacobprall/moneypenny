export type IdeaSource = "global" | "repo";

export interface IdeaLink {
  type: string;
  id: string;
  note?: string;
}

export interface Idea {
  filename: string;
  path: string;
  source: IdeaSource;
  title: string;
  status: string;
  priority?: string;
  tags?: string[];
  spec_session_id?: string | null;
  impl_session_ids?: string[];
  created_at?: string;
  updated_at?: string;
  links?: IdeaLink[];
  extra: Record<string, unknown>;
  body: string;
}
