import { invoke } from "@tauri-apps/api/core";

// Mirrors `crates/secunit-gui/src/projects.rs#ProjectsView`. We hand-roll
// these for now; JOB-03 introduces ts-rs-generated bindings for the
// larger registry surface.
export interface ProjectEntryView {
  name: string;
  path: string;
  resolved_path: string;
  exists: boolean;
}

export interface ProjectsView {
  projects: ProjectEntryView[];
  default: string | null;
  last_selected: string | null;
  config_path: string;
}

export async function listProjects(): Promise<ProjectsView> {
  return invoke<ProjectsView>("list_projects");
}

export async function selectProject(name: string): Promise<string> {
  return invoke<string>("select_project", { name });
}

export async function currentProject(): Promise<string | null> {
  return invoke<string | null>("current_project");
}
