import { useEffect, useState } from "react";
import { api, type Project } from "../api";
import { useT } from "../i18n";

/**
 * Naming and colouring the projects the panel has seen.
 *
 * Identity is keyed on the folder, not the session, so a rename survives
 * restarts and applies to every future session in that directory.
 */
export function ProjectsView() {
  const t = useT();
  const [projects, setProjects] = useState<Project[]>([]);
  const [palette, setPalette] = useState<string[]>([]);
  const [defaultCommand, setDefaultCommand] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  useEffect(() => {
    void api.listProjects().then(setProjects);
    void api.palette().then(setPalette);
    void api.defaultOpenCommand().then(setDefaultCommand);
  }, []);

  // Only the field being edited changes; the others keep whatever override
  // they already had rather than being reset to the derived value.
  const overrides = (project: Project) => ({
    name: project.label === folderName(project.cwd) ? null : project.label,
    color: project.customized ? project.color : null,
    command: project.openCommand || null,
  });

  const rename = (project: Project, name: string) => {
    const o = overrides(project);
    void api
      .setProject(project.cwd, name.trim() || null, o.color, o.command)
      .then(setProjects);
  };

  const recolor = (project: Project, color: string) => {
    const o = overrides(project);
    void api.setProject(project.cwd, o.name, color, o.command).then(setProjects);
  };

  const setCommand = (project: Project, command: string) => {
    const o = overrides(project);
    void api.setProject(project.cwd, o.name, o.color, command || null).then(setProjects);
  };

  const reset = (project: Project) => {
    void api.setProject(project.cwd, null, null, null).then(setProjects);
  };

  return (
    <div className="projects">
      <h2>{t.projects.title}</h2>
      {projects.length === 0 ? (
        <p className="note">{t.projects.emptyHint}</p>
      ) : (
        <ul>
          {projects.map((project) => (
            <li key={project.cwd}>
              <div className="project-head">
                <span className="swatch" style={{ background: project.color }} />
                {editing === project.cwd ? (
                  <input
                    autoFocus
                    className="rename"
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onBlur={() => {
                      rename(project, draft);
                      setEditing(null);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") e.currentTarget.blur();
                      if (e.key === "Escape") setEditing(null);
                      e.stopPropagation();
                    }}
                  />
                ) : (
                  <button
                    className="link"
                    onClick={() => {
                      setEditing(project.cwd);
                      setDraft(project.label);
                    }}
                  >
                    {project.label}
                  </button>
                )}
                {project.customized && (
                  <button className="reset" onClick={() => reset(project)}>
                    {t.projects.resetToDefault}
                  </button>
                )}
              </div>
              <p className="path">{project.cwd}</p>
              <div className="swatches">
                {palette.map((color) => (
                  <button
                    key={color}
                    className={`swatch pick ${color === project.color ? "on" : ""}`}
                    style={{ background: color }}
                    title={color}
                    onClick={() => recolor(project, color)}
                  />
                ))}
              </div>
              <div className="command">
                <span>
                  {t.projects.command}
                  <em>{t.projects.commandHint}</em>
                </span>
                <input
                  // Remounts when the stored value changes, so the presets
                  // below actually show up in the field.
                  key={project.openCommand}
                  defaultValue={project.openCommand}
                  placeholder={defaultCommand}
                  onBlur={(e) => setCommand(project, e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") e.currentTarget.blur();
                    e.stopPropagation();
                  }}
                />
                <div className="presets">
                  <button onClick={() => setCommand(project, "")}>VS Code</button>
                  <button onClick={() => setCommand(project, 'wt -d "{cwd}"')}>
                    Windows Terminal
                  </button>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function folderName(cwd: string): string {
  return cwd.split(/[/\\]/).filter(Boolean).pop() ?? cwd;
}
