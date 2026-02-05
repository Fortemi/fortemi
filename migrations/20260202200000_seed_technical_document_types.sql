-- Seed Technical Document Types
-- Issues #397-#403: API specs, IaC, database schemas, shell scripts, docs formats, package configs, observability

-- #397: API Specifications
INSERT INTO document_type (name, display_name, category, description, file_extensions, magic_patterns, filename_patterns, chunking_strategy, tree_sitter_language, is_system) VALUES
('openapi', 'OpenAPI Specification', 'api-spec', 'OpenAPI/Swagger API specifications', ARRAY['.yaml', '.yml', '.json'], ARRAY['openapi:', 'swagger:'], ARRAY['openapi.yaml', 'openapi.yml', 'openapi.json', 'swagger.yaml', 'swagger.yml', 'swagger.json'], 'per_section', 'yaml', TRUE),
('graphql-schema', 'GraphQL Schema', 'api-spec', 'GraphQL schema definition language', ARRAY['.graphql', '.gql'], ARRAY['type Query', 'type Mutation', 'schema {'], ARRAY['schema.graphql', 'schema.gql'], 'per_unit', NULL, TRUE),
('protobuf', 'Protocol Buffers', 'api-spec', 'Protocol Buffer schema definitions', ARRAY['.proto'], ARRAY['syntax = "proto3"', 'syntax = "proto2"', 'message ', 'service '], NULL, 'per_unit', NULL, TRUE),
('asyncapi', 'AsyncAPI Specification', 'api-spec', 'AsyncAPI event-driven API specifications', ARRAY['.yaml', '.yml', '.json'], ARRAY['asyncapi:'], ARRAY['asyncapi.yaml', 'asyncapi.yml', 'asyncapi.json'], 'per_section', 'yaml', TRUE),
('json-schema', 'JSON Schema', 'api-spec', 'JSON Schema validation definitions', ARRAY['.json', '.schema.json'], ARRAY['"$schema":', '"definitions":', '"properties":'], ARRAY['*.schema.json'], 'fixed', 'json', TRUE),

-- #398: Infrastructure as Code
('terraform', 'Terraform', 'iac', 'Terraform infrastructure definitions', ARRAY['.tf', '.tfvars'], ARRAY['resource "', 'provider "', 'module "', 'variable "'], ARRAY['*.tf', '*.tfvars', 'terraform.tfvars'], 'per_unit', 'hcl', TRUE),
('kubernetes', 'Kubernetes', 'iac', 'Kubernetes resource manifests', ARRAY['.yaml', '.yml'], ARRAY['apiVersion:', 'kind: Deployment', 'kind: Service', 'kind: Pod', 'kind: ConfigMap'], ARRAY['*.k8s.yaml', '*.k8s.yml', 'deployment.yaml', 'service.yaml'], 'per_section', 'yaml', TRUE),
('dockerfile', 'Dockerfile', 'iac', 'Docker container definitions', ARRAY['.dockerfile'], ARRAY['FROM ', 'RUN ', 'COPY ', 'WORKDIR ', 'ENV '], ARRAY['Dockerfile', 'Dockerfile.*', '*.dockerfile'], 'per_unit', 'dockerfile', TRUE),
('docker-compose', 'Docker Compose', 'iac', 'Docker Compose multi-container definitions', ARRAY['.yaml', '.yml'], ARRAY['version:', 'services:', 'volumes:', 'networks:'], ARRAY['docker-compose.yaml', 'docker-compose.yml', 'compose.yaml', 'compose.yml'], 'per_section', 'yaml', TRUE),
('ansible', 'Ansible', 'iac', 'Ansible automation playbooks', ARRAY['.yaml', '.yml'], ARRAY['- hosts:', 'tasks:', 'roles:', 'playbook:', '- name:'], ARRAY['playbook.yaml', 'playbook.yml', 'site.yml'], 'per_section', 'yaml', TRUE),
('cloudformation', 'CloudFormation', 'iac', 'AWS CloudFormation templates', ARRAY['.yaml', '.yml', '.json'], ARRAY['AWSTemplateFormatVersion:', 'Resources:', 'Parameters:', 'Outputs:'], ARRAY['template.yaml', 'template.yml', 'cloudformation.yaml'], 'per_section', 'yaml', TRUE),
('helm', 'Helm Chart', 'iac', 'Kubernetes Helm chart templates', ARRAY['.yaml', '.yml', '.tpl'], ARRAY['{{ .Values.', '{{- include ', 'apiVersion:', '{{- range'], ARRAY['Chart.yaml', 'values.yaml', '*.tpl'], 'per_section', 'yaml', TRUE),

-- #399: Database & Schema
('sql-migration', 'SQL Migration', 'database', 'Database migration scripts', ARRAY['.sql'], ARRAY['CREATE TABLE', 'ALTER TABLE', 'DROP TABLE', 'CREATE INDEX', 'migrations/', 'migrate/'], ARRAY['*_migration.sql', '*_migrate.sql', '*.up.sql', '*.down.sql'], 'per_unit', NULL, TRUE),
('prisma', 'Prisma Schema', 'database', 'Prisma ORM schema definitions', ARRAY['.prisma'], ARRAY['datasource ', 'generator ', 'model ', 'enum '], ARRAY['schema.prisma', 'prisma.schema'], 'per_unit', NULL, TRUE),
('drizzle', 'Drizzle Schema', 'database', 'Drizzle ORM schema definitions', ARRAY['.ts', '.js'], ARRAY['import { pgTable', 'import { mysqlTable', 'import { sqliteTable', 'drizzle-orm'], ARRAY['schema.ts', 'drizzle.config.ts'], 'syntactic', 'typescript', TRUE),
('sqlalchemy', 'SQLAlchemy Models', 'database', 'SQLAlchemy ORM model definitions', ARRAY['.py'], ARRAY['from sqlalchemy import', 'Base = declarative_base()', 'class Meta:', '__tablename__'], ARRAY['models.py', 'model.py'], 'syntactic', 'python', TRUE),
('erd', 'Entity Relationship Diagram', 'database', 'Database ERD definitions (Mermaid, PlantUML)', ARRAY['.mmd', '.mermaid', '.puml', '.plantuml'], ARRAY['erDiagram', '@startuml', 'entity ', 'relationship '], NULL, 'per_section', NULL, TRUE),

-- #400: Shell & Build Scripts
('bash', 'Bash Script', 'shell', 'Bash shell scripts', ARRAY['.sh', '.bash'], ARRAY['#!/bin/bash', '#!/usr/bin/env bash'], ARRAY['*.sh', '*.bash'], 'per_unit', 'bash', TRUE),
('zsh', 'Zsh Script', 'shell', 'Zsh shell scripts', ARRAY['.zsh'], ARRAY['#!/bin/zsh', '#!/usr/bin/env zsh'], ARRAY['*.zsh', '.zshrc', '.zprofile'], 'per_unit', 'bash', TRUE),
('powershell', 'PowerShell', 'shell', 'PowerShell scripts', ARRAY['.ps1', '.psm1', '.psd1'], ARRAY['param(', 'function ', 'Write-Host', 'Get-', 'Set-'], NULL, 'per_unit', NULL, TRUE),
('makefile', 'Makefile', 'shell', 'Make build automation scripts', NULL, ARRAY['.PHONY:', 'all:', 'clean:', 'build:', '\t@'], ARRAY['Makefile', 'makefile', 'GNUmakefile'], 'per_unit', NULL, TRUE),
('justfile', 'Justfile', 'shell', 'Just command runner scripts', NULL, ARRAY['@echo', '[private]', 'default:', 'alias '], ARRAY['Justfile', 'justfile', '.justfile'], 'per_unit', NULL, TRUE),
('cmake', 'CMake', 'shell', 'CMake build system scripts', ARRAY['.cmake'], ARRAY['cmake_minimum_required', 'project(', 'add_executable', 'add_library'], ARRAY['CMakeLists.txt', '*.cmake'], 'per_unit', NULL, TRUE),
('gradle', 'Gradle', 'shell', 'Gradle build scripts', ARRAY['.gradle', '.gradle.kts'], ARRAY['plugins {', 'dependencies {', 'repositories {', 'task ', 'apply plugin:'], ARRAY['build.gradle', 'build.gradle.kts', 'settings.gradle'], 'per_unit', NULL, TRUE),

-- #401: Documentation Formats
('rst', 'reStructuredText', 'docs', 'reStructuredText documentation', ARRAY['.rst'], ARRAY['.. _', '.. code-block::', '.. toctree::', '===', '---'], ARRAY['*.rst', 'index.rst'], 'semantic', NULL, TRUE),
('asciidoc', 'AsciiDoc', 'docs', 'AsciiDoc documentation', ARRAY['.adoc', '.asciidoc', '.asc'], ARRAY['= ', '== ', '=== ', '[source,', '----'], ARRAY['*.adoc', 'README.adoc'], 'semantic', NULL, TRUE),
('org-mode', 'Org Mode', 'docs', 'Emacs Org-mode documents', ARRAY['.org'], ARRAY['#+TITLE:', '#+BEGIN_', '#+END_', '* ', '** '], NULL, 'semantic', NULL, TRUE),
('latex', 'LaTeX', 'docs', 'LaTeX typesetting documents', ARRAY['.tex', '.latex'], ARRAY['\documentclass', '\begin{document}', '\section{', '\subsection{', '\usepackage'], NULL, 'per_section', 'latex', TRUE),
('man-page', 'Man Page', 'docs', 'Unix manual page (troff/groff)', ARRAY['.1', '.2', '.3', '.4', '.5', '.6', '.7', '.8', '.man'], ARRAY['.TH ', '.SH ', '.B ', '.I '], ARRAY['*.man'], 'per_section', NULL, TRUE),
('jupyter', 'Jupyter Notebook', 'docs', 'Jupyter notebook documents', ARRAY['.ipynb'], ARRAY['"nbformat":', '"cells":', '"cell_type": "code"', '"cell_type": "markdown"'], NULL, 'per_section', 'json', TRUE),
('mdx', 'MDX', 'docs', 'Markdown with JSX components', ARRAY['.mdx'], ARRAY['import ', 'export ', '<'], NULL, 'semantic', NULL, TRUE),
('docstring', 'Docstring', 'docs', 'Extracted API documentation', ARRAY['.docstring'], ARRAY['/**', '"""', E'\'\'\'', '///', '<!---'], NULL, 'per_unit', NULL, TRUE),

-- #402: Package & Build Configs
('cargo-toml', 'Cargo.toml', 'package', 'Rust package manifest', ARRAY['.toml'], ARRAY['[package]', '[dependencies]', 'name = ', 'version = '], ARRAY['Cargo.toml'], 'per_section', 'toml', TRUE),
('package-json', 'package.json', 'package', 'Node.js package manifest', ARRAY['.json'], ARRAY['"name":', '"version":', '"dependencies":', '"scripts":'], ARRAY['package.json'], 'fixed', 'json', TRUE),
('pyproject', 'pyproject.toml', 'package', 'Python project configuration', ARRAY['.toml'], ARRAY['[tool.poetry]', '[build-system]', '[project]', 'requires-python'], ARRAY['pyproject.toml'], 'per_section', 'toml', TRUE),
('go-mod', 'go.mod', 'package', 'Go module definition', ARRAY['.mod'], ARRAY['module ', 'go ', 'require (', 'replace '], ARRAY['go.mod'], 'per_section', NULL, TRUE),
('pom-xml', 'pom.xml', 'package', 'Maven project object model', ARRAY['.xml'], ARRAY['<project', '<dependencies>', '<groupId>', '<artifactId>'], ARRAY['pom.xml'], 'per_section', 'xml', TRUE),
('gemfile', 'Gemfile', 'package', 'Ruby dependencies', NULL, ARRAY['source ', 'gem ', 'group :'], ARRAY['Gemfile'], 'per_unit', 'ruby', TRUE),
('requirements', 'requirements.txt', 'package', 'Python dependencies', ARRAY['.txt'], ARRAY['==', '>=', '~=', '-r ', '--hash'], ARRAY['requirements.txt', 'requirements-*.txt', 'requirements/*.txt'], 'fixed', NULL, TRUE),
('lockfile', 'Lock File', 'package', 'Dependency lock files', ARRAY['.lock', '.frozen'], ARRAY['"lockfileVersion":', 'packages:', '[[package]]'], ARRAY['package-lock.json', 'yarn.lock', 'Cargo.lock', 'poetry.lock', 'Gemfile.lock', 'pnpm-lock.yaml'], 'whole', NULL, TRUE),

-- #403: Logs & Observability
('log-file', 'Log File', 'observability', 'Application log files', ARRAY['.log'], ARRAY['ERROR', 'WARN', 'INFO', 'DEBUG', 'TRACE', '[error]', '[warn]'], ARRAY['*.log', '*.log.*', 'app.log'], 'fixed', NULL, TRUE),
('stack-trace', 'Stack Trace', 'observability', 'Exception stack traces', ARRAY['.trace', '.stack'], ARRAY['Traceback', 'at ', 'Caused by:', 'Exception in thread', 'panicked at'], NULL, 'whole', NULL, TRUE),
('error-report', 'Error Report', 'observability', 'Structured error reports', ARRAY['.err', '.error'], ARRAY['"error":', '"exception":', '"stack":', '"message":'], NULL, 'whole', 'json', TRUE),
('metrics', 'Metrics', 'observability', 'Time-series metrics data', ARRAY['.metrics', '.prom'], ARRAY['# HELP', '# TYPE', '_total', '_count', '_sum'], NULL, 'fixed', NULL, TRUE),
('trace-json', 'Trace JSON', 'observability', 'Distributed trace spans (OpenTelemetry, Jaeger)', ARRAY['.json'], ARRAY['"traceId":', '"spanId":', '"operationName":', '"startTime":', '"duration":'], ARRAY['trace.json', 'traces.json'], 'per_section', 'json', TRUE);
