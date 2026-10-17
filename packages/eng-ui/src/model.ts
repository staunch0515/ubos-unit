// The model of a project as `forge types` writes it (`generated/model.json`): classes with
// their fields and ports, used to generate forms and table columns.

export interface FieldDecl {
  name: string;
  type: string;
  required: boolean;
  label: string;
  constraints: { allowed_values?: string[]; max_length?: number; target_class?: string } | null;
  default: unknown;
  classification: string | null;
  declared_by: string;
}

export interface PortDecl {
  name: string;
  kind: string;
  params: { name: string; type?: string; required?: boolean; label?: string }[];
  declared_by: string;
}

export interface ClassDecl {
  name: string;
  label: string;
  parent: string | null;
  kind: string | null;
  linearization: string[];
  fields: FieldDecl[];
  ports: PortDecl[];
}

export interface Model {
  project: string;
  branch: string;
  model_hash: string;
  vae: string;
  classes: ClassDecl[];
}

/** Validate the shape of a loaded model. */
export function parseModel(text: string): Model {
  const m = JSON.parse(text) as Model;
  if (!Array.isArray(m.classes)) throw new Error("not a model.json of forge types: no classes");
  return m;
}

/** The classes as a tree: roots are the classes whose parent is not in the model. */
export function tree(m: Model): { cls: ClassDecl; depth: number }[] {
  const names = new Set(m.classes.map((c) => c.name));
  const out: { cls: ClassDecl; depth: number }[] = [];
  const visit = (c: ClassDecl, depth: number) => {
    out.push({ cls: c, depth });
    for (const child of m.classes.filter((x) => x.parent === c.name)) visit(child, depth + 1);
  };
  for (const c of m.classes.filter((x) => !x.parent || !names.has(x.parent))) visit(c, 0);
  return out;
}
