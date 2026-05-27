import type { MeshModelRaw } from '@/lib/api/types'
import type { ModelSummary } from '@/features/app-tabs/types'

type ModelRecord = Partial<MeshModelRaw> & {
  id?: unknown
  model?: unknown
}

type ModelsPayload = {
  mesh_models?: unknown
  models?: unknown
  data?: unknown
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

function modelRecords(input: unknown): ModelRecord[] {
  if (Array.isArray(input)) return input.filter(isRecord) as ModelRecord[]
  if (!isRecord(input)) return []

  const payload = input as ModelsPayload
  const candidates = [payload.mesh_models, payload.models, payload.data]
  const models = candidates.find(Array.isArray)
  return Array.isArray(models) ? (models.filter(isRecord) as ModelRecord[]) : []
}

function modelName(model: ModelRecord): string | undefined {
  if (typeof model.name === 'string' && model.name.trim()) return model.name
  if (typeof model.id === 'string' && model.id.trim()) return model.id
  if (typeof model.model === 'string' && model.model.trim()) return model.model
  return undefined
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : []
}

function formatSize(sizeGB: number | undefined): string {
  if (sizeGB == null) return 'Unknown'

  if (sizeGB >= 1) {
    return `${sizeGB.toFixed(1)}B`
  }
  return `${(sizeGB * 1000).toFixed(0)}M`
}

function formatContext(contextLength: number | undefined): string {
  if (contextLength == null) return 'Unknown'

  const k = Math.round(contextLength / 1000)
  return `${k}K`
}

function mapModelStatus(status: ModelRecord['status']): ModelSummary['status'] {
  if (status === 'warm') return 'warm'
  return 'offline'
}

export function adaptModelsToSummary(input: unknown): ModelSummary[] {
  return modelRecords(input).flatMap((model) => {
    const name = modelName(model)
    if (!name) return []
    const tags = stringArray(model.tags)

    return [
      {
        name,
        family: model.family ?? name.split('/')[0] ?? 'unknown',
        size: formatSize(model.size_gb),
        context: formatContext(model.context_length),
        status: mapModelStatus(model.status),
        tags,
        nodeCount: model.node_count,
        fullId: name,
        paramsB: model.params_b,
        paramsLabel: model.params_b != null ? `${model.params_b}B` : undefined,
        quant: model.quantization,
        sizeGB: model.size_gb,
        diskGB: model.disk_gb,
        ctxMaxK: model.context_length == null ? undefined : Math.round(model.context_length / 1000),
        moe: model.capabilities?.moe ?? model.moe ?? false,
        vision: model.capabilities?.vision ?? model.vision ?? tags.includes('vision'),
        capabilities: model.capabilities,
        license: model.license
      }
    ]
  })
}
