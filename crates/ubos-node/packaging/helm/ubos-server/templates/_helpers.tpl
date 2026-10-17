{{- define "ubos.name" -}}{{ .Release.Name }}-ubos-server{{- end -}}
{{- define "ubos.labels" -}}
app.kubernetes.io/name: ubos-server
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}
{{- define "ubos.volumes" -}}
- name: config
  configMap: {name: {{ include "ubos.name" . }}}
- name: keys
  secret: {secretName: {{ .Values.existingSecret }}, defaultMode: 0440}
{{- if .Values.backup.enabled }}
- name: backups
  persistentVolumeClaim: {claimName: {{ include "ubos.name" . }}-backups}
{{- end }}
{{- end -}}
{{- define "ubos.mounts" -}}
- {name: config, mountPath: /etc/ubos/ubos.toml, subPath: ubos.toml}
- {name: keys, mountPath: /etc/ubos/keys, readOnly: true}
{{- if .Values.backup.enabled }}
- {name: backups, mountPath: /var/backups/ubos}
{{- end }}
{{- end -}}
