{{- define "nimbus.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "nimbus.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "nimbus.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "nimbus.selectorLabels" -}}
app.kubernetes.io/name: {{ include "nimbus.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "nimbus.labels" -}}
helm.sh/chart: {{ include "nimbus.chart" . }}
{{ include "nimbus.selectorLabels" . }}
app.kubernetes.io/version: {{ .Values.image.tag | default .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "nimbus.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "nimbus.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{- define "nimbus.image" -}}
{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}
{{- end }}

{{/* Environment shared by the application and the migration Job. */}}
{{- define "nimbus.env" -}}
- name: PAM_ENV
  value: production
- name: PAM_HOST
  value: "0.0.0.0"
- name: PAM_PORT
  value: {{ .Values.port | quote }}
{{- if .Values.metrics.enabled }}
- name: NIMBUS_METRICS
  value: "1"
- name: NIMBUS_METRICS_PATH
  value: {{ .Values.metrics.path | quote }}
{{- end }}
{{- range $name, $value := .Values.env }}
- name: {{ $name }}
  value: {{ $value | quote }}
{{- end }}
{{- with .Values.extraEnv }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{- define "nimbus.envFrom" -}}
{{- if .Values.secretEnv }}
- secretRef:
    name: {{ include "nimbus.fullname" . }}
{{- end }}
{{- if .Values.existingSecret }}
- secretRef:
    name: {{ .Values.existingSecret }}
{{- end }}
{{- with .Values.extraEnvFrom }}
{{ toYaml . }}
{{- end }}
{{- end }}
