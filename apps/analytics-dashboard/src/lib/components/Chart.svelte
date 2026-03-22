<script lang="ts">
    import { onMount } from 'svelte'
    import uPlot from 'uplot'
    import 'uplot/dist/uPlot.min.css'

    interface Props {
        /** Array of [timestamps, ...valueSeries] in uPlot format. Timestamps are unix seconds. */
        data: uPlot.AlignedData
        /** Series labels (excluding the timestamp series). */
        labels?: string[]
        /** Chart height in pixels. */
        height?: number
    }

    let { data, labels = [], height = 200 }: Props = $props()

    let container: HTMLDivElement
    let chart: uPlot | null = null

    function buildOpts(width: number): uPlot.Options {
        const series: uPlot.Series[] = [
            {}, // timestamp series (x-axis)
            ...labels.map((label, i) => ({
                label,
                stroke: i === 0 ? '#ffc206' : '#a1a1aa',
                width: 2,
                fill: i === 0 ? 'rgba(255, 194, 6, 0.08)' : undefined,
            })),
        ]

        // If no labels provided, add a default series
        if (labels.length === 0 && data.length > 1) {
            for (let i = 1; i < data.length; i++) {
                series.push({
                    stroke: i === 1 ? '#ffc206' : '#a1a1aa',
                    width: 2,
                    fill: i === 1 ? 'rgba(255, 194, 6, 0.08)' : undefined,
                })
            }
        }

        return {
            width,
            height,
            series,
            axes: [
                {
                    stroke: '#a1a1aa',
                    grid: { stroke: 'rgba(46, 45, 42, 0.6)', width: 1 },
                    ticks: { stroke: 'rgba(46, 45, 42, 0.6)', width: 1 },
                    font: '11px -apple-system, system-ui, sans-serif',
                },
                {
                    stroke: '#a1a1aa',
                    grid: { stroke: 'rgba(46, 45, 42, 0.6)', width: 1 },
                    ticks: { stroke: 'rgba(46, 45, 42, 0.6)', width: 1 },
                    font: '11px -apple-system, system-ui, sans-serif',
                    size: 50,
                },
            ],
            cursor: {
                drag: { x: false, y: false },
            },
            legend: {
                show: labels.length > 1,
            },
            padding: [8, 8, 0, 0],
        }
    }

    function createChart() {
        if (!container || data[0].length === 0) return
        chart?.destroy()
        const opts = buildOpts(container.clientWidth)
        chart = new uPlot(opts, data, container)
    }

    onMount(() => {
        const resizeObserver = new ResizeObserver((entries) => {
            for (const entry of entries) {
                if (chart) {
                    chart.setSize({ width: entry.contentRect.width, height })
                }
            }
        })
        resizeObserver.observe(container)

        return () => {
            resizeObserver.disconnect()
            chart?.destroy()
            chart = null
        }
    })

    // Recreate chart when data changes
    $effect(() => {
        // Touch data to subscribe to it
        void data
        if (container) createChart()
    })
</script>

<div bind:this={container} class="w-full"></div>
