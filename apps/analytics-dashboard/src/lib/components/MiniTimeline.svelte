<script lang="ts">
    import { onMount } from 'svelte'
    import uPlot from 'uplot'
    import 'uplot/dist/uPlot.min.css'

    interface Props {
        /** uPlot AlignedData: [timestamps[], values[]] */
        data: uPlot.AlignedData
        height?: number
        /** Fixed Y-axis max for consistent scale across charts. */
        maxY?: number
        /** Shared zoom X-axis min (unix seconds). null = auto. */
        xMin?: number | null
        /** Shared zoom X-axis max (unix seconds). null = auto. */
        xMax?: number | null
        /** Cursor sync key — charts with the same key sync crosshairs. */
        syncKey?: string
        /** Fires when the cursor hovers a data point (index into data[0]) or leaves (null). */
        onhover?: (idx: number | null) => void
    }

    let { data, height = 48, maxY, xMin = null, xMax = null, syncKey, onhover }: Props = $props()

    let container: HTMLDivElement
    let chart: uPlot | null = null

    function buildOpts(width: number): uPlot.Options {
        const yMax = maxY ?? Math.max(...(data[1] as number[]), 1)

        const opts: uPlot.Options = {
            width,
            height,
            series: [
                {},
                {
                    stroke: '#ffc206',
                    width: 1.5,
                    fill: 'rgba(255, 194, 6, 0.08)',
                },
            ],
            axes: [
                {
                    stroke: '#71717a',
                    font: '9px -apple-system, system-ui, sans-serif',
                    ticks: { show: false },
                    grid: { show: false },
                    gap: 2,
                    size: 14,
                },
                { show: false },
            ],
            scales: {
                y: { range: () => [0, yMax] },
            },
            cursor: syncKey
                ? {
                      show: true,
                      x: true,
                      y: false,
                      points: { show: false },
                      sync: { key: syncKey, setSeries: false },
                  }
                : { show: false },
            legend: { show: false },
            padding: [2, 0, 0, 0],
        }

        if (onhover) {
            opts.hooks = {
                setCursor: [
                    (u: uPlot) => {
                        onhover(u.cursor.idx ?? null)
                    },
                ],
            }
        }

        return opts
    }

    function createChart() {
        if (!container || data[0].length === 0) return
        chart?.destroy()
        chart = new uPlot(buildOpts(container.clientWidth), data, container)
        if (xMin != null && xMax != null) {
            chart.setScale('x', { min: xMin, max: xMax })
        }
    }

    onMount(() => {
        const ro = new ResizeObserver((entries) => {
            for (const entry of entries) {
                chart?.setSize({ width: entry.contentRect.width, height })
            }
        })
        ro.observe(container)
        return () => {
            ro.disconnect()
            chart?.destroy()
            chart = null
        }
    })

    $effect(() => {
        void data
        void maxY
        if (container) createChart()
    })

    $effect(() => {
        if (chart && xMin != null && xMax != null) {
            chart.setScale('x', { min: xMin, max: xMax })
        } else if (chart && xMin == null && xMax == null && data[0].length > 0) {
            chart.setScale('x', {
                min: data[0][0] as number,
                max: data[0][data[0].length - 1] as number,
            })
        }
    })
</script>

<div bind:this={container} class="w-full"></div>
