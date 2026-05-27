import { describe, expect, it } from 'vitest'
import { adaptModelsToSummary } from '@/features/network/api/models-adapter'
import type { MeshModelRaw } from '@/lib/api/types'

const WARM_MODEL: MeshModelRaw = {
  name: 'org/warm-model',
  status: 'warm',
  size_gb: 7,
  node_count: 2,
  quantization: 'Q4_K_M',
  context_length: 8192,
  family: 'org',
  tags: ['text'],
  params_b: 7,
  disk_gb: 4.5,
  capabilities: { vision: false, moe: false },
  license: 'test-license'
}

describe('adaptModelsToSummary', () => {
  it('accepts public API model rows without a nested capabilities object', () => {
    const models: MeshModelRaw[] = [
      {
        name: 'Hermes-2-Pro-Mistral-7B-Q4_K_M',
        status: 'warm',
        size_gb: 4.4,
        node_count: 1,
        quantization: 'Q4_K_M',
        moe: false,
        vision: false
      }
    ]

    expect(adaptModelsToSummary(models)).toEqual([
      expect.objectContaining({
        name: 'Hermes-2-Pro-Mistral-7B-Q4_K_M',
        status: 'warm',
        size: '4.4B',
        context: 'Unknown',
        ctxMaxK: undefined,
        moe: false,
        vision: false
      })
    ])
  })

  it('maps a valid mesh_models payload', () => {
    expect(adaptModelsToSummary({ mesh_models: [WARM_MODEL] })).toEqual([
      expect.objectContaining({
        name: 'org/warm-model',
        family: 'org',
        size: '7.0B',
        context: '8K',
        status: 'warm',
        tags: ['text'],
        nodeCount: 2,
        fullId: 'org/warm-model',
        paramsB: 7,
        paramsLabel: '7B',
        quant: 'Q4_K_M',
        sizeGB: 7,
        diskGB: 4.5,
        ctxMaxK: 8,
        moe: false,
        vision: false,
        license: 'test-license'
      })
    ])
  })

  it('prefers nested capabilities when available', () => {
    const models: MeshModelRaw[] = [
      {
        name: 'Qwen3-VL-8B-Q4_K_M',
        status: 'cold',
        size_gb: 5,
        node_count: 0,
        capabilities: { moe: true, vision: true },
        quantization: 'Q4_K_M',
        context_length: 128_000,
        moe: false,
        vision: false
      }
    ]

    expect(adaptModelsToSummary(models)[0]).toEqual(
      expect.objectContaining({
        status: 'offline',
        context: '128K',
        ctxMaxK: 128,
        moe: true,
        vision: true
      })
    )
  })

  it('accepts OpenAI-style data arrays without crashing', () => {
    expect(adaptModelsToSummary({ data: [{ id: 'org/openai-listed-model' }] })).toEqual([
      expect.objectContaining({
        name: 'org/openai-listed-model',
        family: 'org',
        size: 'Unknown',
        context: 'Unknown',
        status: 'offline',
        tags: [],
        fullId: 'org/openai-listed-model'
      })
    ])
  })

  it('accepts models arrays without crashing', () => {
    expect(adaptModelsToSummary({ models: [{ name: 'local/model', status: 'cold', node_count: 1 }] })).toEqual([
      expect.objectContaining({
        name: 'local/model',
        family: 'local',
        status: 'offline'
      })
    ])
  })

  it('returns an empty summary for missing model arrays', () => {
    expect(adaptModelsToSummary({ object: 'list' })).toEqual([])
  })

  it('returns an empty summary for undefined, null, and empty arrays', () => {
    expect(adaptModelsToSummary(undefined)).toEqual([])
    expect(adaptModelsToSummary(null)).toEqual([])
    expect(adaptModelsToSummary([])).toEqual([])
  })
})
