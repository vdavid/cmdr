<script lang="ts">
    import { onMount } from 'svelte'
    import uPlot from 'uplot'
    import 'uplot/dist/uPlot.min.css'

    interface Props {
        /** Array of [timestamps, ...valueSeries] in uPlot format. Timestamps are unix seconds. */
        data: uPlot.AlignedData
        /** Series labels (excluding the timestamp series). */
        labels?: string[]
        /** Custom stroke colors per series. Falls back to gold/grey. */
        colors?: string[]
        /** Chart height in pixels. */
        height?: number
        /** Default X-axis min (unix seconds). Scroll to zoom overrides this. */
        xMin?: number | null
        /** Default X-axis max (unix seconds). Scroll to zoom overrides this. */
        xMax?: number | null
    }

    let { data, labels = [], colors = [], height = 200, xMin = null, xMax = null }: Props = $props()

    let zoomedXMin: number | null = $state(null)
    let zoomedXMax: number | null = $state(null)

    // Reset zoom when external xMin/xMax change (e.g. range switch)
    $effect(() => {
        void xMin
        void xMax
        zoomedXMin = null
        zoomedXMax = null
    })

    let container: HTMLDivElement
    let chart: uPlot | null = null

    const defaultColors = ['#ffc206', '#a1a1aa', '#8b5cf6', '#10b981']

    function colorAt(i: number): string {
        return colors[i] ?? defaultColors[i] ?? '#a1a1aa'
    }

    function handleWheel(e: WheelEvent) {
        e.preventDefault()
        if (data[0].length < 2) return
        const dataXMin = data[0][0] as number
        const dataXMax = data[0][data[0].length - 1] as number
        const zoomFactor = e.deltaY > 0 ? 1.3 : 1 / 1.3
        const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
        const fraction = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width))

        const curMin = zoomedXMin ?? xMin ?? dataXMin
        const curMax = zoomedXMax ?? xMax ?? dataXMax
        const range = curMax - curMin
        const center = curMin + range * fraction
        const newRange = Math.min(range * zoomFactor, dataXMax - dataXMin)

        if (newRange >= dataXMax - dataXMin) {
            zoomedXMin = null
            zoomedXMax = null
        } else {
            zoomedXMin = Math.max(dataXMin, center - newRange * fraction)
            zoomedXMax = Math.min(dataXMax, zoomedXMin + newRange)
        }
    }

    function buildOpts(width: number): uPlot.Options {
        const series: uPlot.Series[] = [
            {}, // timestamp series (x-axis)
            ...labels.map((label, i) => ({
                label,
                stroke: colorAt(i),
                width: 2,
                fill: colorAt(i) + '14', // ~8% opacity
            })),
        ]

        // If no labels provided, add a default series
        if (labels.length === 0 && data.length > 1) {
            for (let i = 1; i < data.length; i++) {
                const c = colorAt(i - 1)
                series.push({
                    stroke: c,
                    width: 2,
                    fill: c + '14',
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

    function applyXScale() {
        if (!chart) return
        const effMin = zoomedXMin ?? xMin
        const effMax = zoomedXMax ?? xMax
        if (effMin != null && effMax != null) {
            chart.setScale('x', { min: effMin, max: effMax })
        } else if (data[0].length > 0) {
            chart.setScale('x', { min: data[0][0] as number, max: data[0][data[0].length - 1] as number })
        }
    }

    function createChart() {
        if (!container || data[0].length === 0) return
        chart?.destroy()
        const opts = buildOpts(container.clientWidth)
        chart = new uPlot(opts, data, container)
        applyXScale()
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
        void data
        if (container) createChart()
    })

    // Apply zoom when it changes
    $effect(() => {
        void zoomedXMin
        void zoomedXMax
        applyXScale()
    })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={container} class="w-full" onwheel={handleWheel}></div>
