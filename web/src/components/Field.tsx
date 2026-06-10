import type { FieldDef, FormValues } from "../params";

interface Props {
  def: FieldDef;
  form: FormValues;
  onChange: (key: string, value: string) => void;
  disabled?: boolean;
}

export function Field({ def, form, onChange, disabled }: Props) {
  const value = form[def.key] ?? "";

  if (def.kind === "bool") {
    const checked = value === "true";
    return (
      <div className="field">
        <label className="toggle">
          <input
            type="checkbox"
            checked={checked}
            disabled={disabled}
            onChange={(e) => onChange(def.key, e.target.checked ? "true" : "false")}
          />
          <span className="track" />
          <span className="toggle-text">{def.label}</span>
        </label>
        {def.help && <span className="field-help">{def.help}</span>}
      </div>
    );
  }

  return (
    <div className="field">
      <span className="field-label">
        {def.label}
        {def.unit && <span className="unit">[{def.unit}]</span>}
      </span>
      {def.kind === "select" ? (
        <select
          value={value}
          disabled={disabled}
          onChange={(e) => onChange(def.key, e.target.value)}
        >
          {def.options?.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      ) : (
        <input
          type={def.kind === "text" ? "text" : "text"}
          inputMode={def.kind === "int" ? "numeric" : "decimal"}
          value={value}
          disabled={disabled}
          spellCheck={false}
          onChange={(e) => onChange(def.key, e.target.value)}
        />
      )}
      {def.help && <span className="field-help">{def.help}</span>}
    </div>
  );
}
