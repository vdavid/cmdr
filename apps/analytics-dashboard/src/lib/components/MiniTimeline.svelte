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
    }

    let { data, height = 100, maxY, xMin = null, xMax = null }: Props = $props()

    let container: HTMLDivElement
    let chart: uPlot | null = null

    function buildOpts(width: number): uPlot.Options {
        const yMax = maxY ?? Math.max(...(data[1] as number[]), 1)
        return {
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
                { show: false },
                { show: false },
            ],
            scales: {
                y: { range: () => [0, yMax] },
            },
            cursor: { show: false },
            legend: { show: false },
            padding: [4, 0, 0, 0],
        }
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
            // Reset to full range
            chart.setScale('x', {
                min: data[0][0] as number,
                max: data[0][data[0].length - 1] as number,
            })
        }
    })
</script>

<div bind:this={container} class="w-full"></div>
