{{/* Shared naming, labels, image and environment helpers for the Fortémi chart. */}}

{{- define "fortemi.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "fortemi.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "fortemi.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "fortemi.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: fortemi
{{- end -}}

{{/* Selector labels; call with (dict "ctx" $ "component" "api"). */}}
{{- define "fortemi.selectorLabels" -}}
app.kubernetes.io/name: {{ include "fortemi.name" .ctx }}
app.kubernetes.io/instance: {{ .ctx.Release.Name }}
app.kubernetes.io/component: {{ .component }}
{{- end -}}

{{- define "fortemi.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "fortemi.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{- define "fortemi.migrationServiceAccountName" -}}
{{- if .Values.migrations.serviceAccount.create -}}
{{- default (printf "%s-migrate" (include "fortemi.fullname" .)) .Values.migrations.serviceAccount.name -}}
{{- else -}}
{{- default (include "fortemi.serviceAccountName" .) .Values.migrations.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/* Image reference; call with (dict "image" <values> "ctx" $ "tagPrefix" ""). Digest wins over tag. */}}
{{- define "fortemi.imageRef" -}}
{{- $tag := default (printf "%s%s" (default "" .tagPrefix) .ctx.Chart.AppVersion) .image.tag -}}
{{- if .image.digest -}}
{{- printf "%s@%s" .image.repository .image.digest -}}
{{- else -}}
{{- printf "%s:%s" .image.repository $tag -}}
{{- end -}}
{{- end -}}

{{- define "fortemi.serverImage" -}}
{{- include "fortemi.imageRef" (dict "image" .Values.image "ctx" .) -}}
{{- end -}}

{{- define "fortemi.multiTenant" -}}
{{- if eq (toString (default "false" (index .Values.env "FORTEMI_MULTI_TENANT"))) "true" -}}true{{- end -}}
{{- end -}}

{{/* Fail fast on configurations the binary would reject at startup. */}}
{{- define "fortemi.validate" -}}
{{- if not .Values.database.existingSecret -}}
{{- fail "database.existingSecret is required (Secret holding DATABASE_URL)" -}}
{{- end -}}
{{- if include "fortemi.multiTenant" . -}}
{{- if not .Values.database.migration.existingSecret -}}
{{- fail "FORTEMI_MULTI_TENANT=true requires database.migration.existingSecret (MIGRATION_DATABASE_URL)" -}}
{{- end -}}
{{- if not .Values.redis.quota.existingSecret -}}
{{- fail "FORTEMI_MULTI_TENANT=true requires redis.quota.existingSecret (FORTEMI_QUOTA_REDIS_URL)" -}}
{{- end -}}
{{- if not (index .Values.env "ISSUER_URL") -}}
{{- fail "FORTEMI_MULTI_TENANT=true requires env.ISSUER_URL" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{/* Database env; call with (dict "ctx" $ "migration" true|false). */}}
{{- define "fortemi.databaseEnv" -}}
{{- $v := .ctx.Values -}}
- name: DATABASE_URL
  valueFrom:
    secretKeyRef:
      name: {{ $v.database.existingSecret }}
      key: {{ $v.database.urlKey }}
{{- if and $v.database.migration.existingSecret (or .migration $v.database.migration.exposeToRuntime) }}
- name: MIGRATION_DATABASE_URL
  valueFrom:
    secretKeyRef:
      name: {{ $v.database.migration.existingSecret }}
      key: {{ $v.database.migration.urlKey }}
{{- end }}
{{- end -}}

{{/* Full server env for API and worker pods. */}}
{{- define "fortemi.serverEnv" -}}
{{- $v := .Values -}}
{{- include "fortemi.databaseEnv" (dict "ctx" . "migration" false) }}
- name: HOST
  value: "0.0.0.0"
- name: FILE_STORAGE_PATH
  value: {{ $v.fileStorage.path | quote }}
- name: REDIS_ENABLED
  value: {{ ternary "true" "false" (and $v.redis.enabled (ne $v.redis.existingSecret "")) | quote }}
{{- if and $v.redis.enabled $v.redis.existingSecret }}
- name: REDIS_URL
  valueFrom:
    secretKeyRef:
      name: {{ $v.redis.existingSecret }}
      key: {{ $v.redis.urlKey }}
{{- end }}
{{- if $v.redis.quota.existingSecret }}
- name: FORTEMI_QUOTA_REDIS_URL
  valueFrom:
    secretKeyRef:
      name: {{ $v.redis.quota.existingSecret }}
      key: {{ $v.redis.quota.urlKey }}
{{- end }}
{{- if $v.authCaBundle.configMap }}
- name: FORTEMI_AUTH_CA_BUNDLE
  value: {{ printf "%s/%s" $v.authCaBundle.mountPath $v.authCaBundle.key | quote }}
{{- end }}
{{- range $name, $value := $v.env }}
{{- if not (has $name (list "WORKER_ENABLED" "PORT" "HOST" "DATABASE_URL" "MIGRATION_DATABASE_URL")) }}
- name: {{ $name }}
  value: {{ toString $value | quote }}
{{- end }}
{{- end }}
{{- with $v.extraEnv }}
{{ toYaml . }}
{{- end }}
{{- end -}}

{{- define "fortemi.serverVolumeMounts" -}}
- name: files
  mountPath: {{ .Values.fileStorage.path }}
- name: tmp
  mountPath: /tmp
{{- if .Values.authCaBundle.configMap }}
- name: oidc-ca
  mountPath: {{ .Values.authCaBundle.mountPath }}
  readOnly: true
{{- end }}
{{- with .Values.extraVolumeMounts }}
{{ toYaml . }}
{{- end }}
{{- end -}}

{{- define "fortemi.serverVolumes" -}}
- name: files
{{- if .Values.fileStorage.existingClaim }}
  persistentVolumeClaim:
    claimName: {{ .Values.fileStorage.existingClaim }}
{{- else }}
  emptyDir: {}
{{- end }}
- name: tmp
  emptyDir: {}
{{- if .Values.authCaBundle.configMap }}
- name: oidc-ca
  configMap:
    name: {{ .Values.authCaBundle.configMap }}
    items:
      - key: {{ .Values.authCaBundle.key }}
        path: {{ .Values.authCaBundle.key }}
{{- end }}
{{- with .Values.extraVolumes }}
{{ toYaml . }}
{{- end }}
{{- end -}}

{{/* HTTP probes for server containers. */}}
{{- define "fortemi.serverProbes" -}}
startupProbe:
  httpGet:
    path: {{ .Values.probes.startup.path }}
    port: http
  periodSeconds: {{ .Values.probes.startup.periodSeconds }}
  failureThreshold: {{ .Values.probes.startup.failureThreshold }}
livenessProbe:
  httpGet:
    path: {{ .Values.probes.liveness.path }}
    port: http
  periodSeconds: {{ .Values.probes.liveness.periodSeconds }}
  failureThreshold: {{ .Values.probes.liveness.failureThreshold }}
readinessProbe:
  httpGet:
    path: {{ .Values.probes.readiness.path }}
    port: http
  periodSeconds: {{ .Values.probes.readiness.periodSeconds }}
  failureThreshold: {{ .Values.probes.readiness.failureThreshold }}
{{- end -}}
