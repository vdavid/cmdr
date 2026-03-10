/**
 * Tests for IPC type guard and error message extraction.
 */

import { describe, it, expect } from 'vitest'
import { isIpcError, getIpcErrorMessage } from './ipc-types'

describe('ipc-types', () => {
    // ========================================================================
    // isIpcError
    // ========================================================================

    describe('isIpcError', () => {
        it('returns true for a valid IpcError', () => {
            expect(isIpcError({ message: 'something went wrong', timedOut: false })).toBe(true)
        })

        it('returns true for a timed-out IpcError', () => {
            expect(isIpcError({ message: 'timeout', timedOut: true })).toBe(true)
        })

        it('returns true when extra properties are present', () => {
            expect(isIpcError({ message: 'err', timedOut: false, extra: 42 })).toBe(true)
        })

        it('returns false for null', () => {
            expect(isIpcError(null)).toBe(false)
        })

        it('returns false for undefined', () => {
            expect(isIpcError(undefined)).toBe(false)
        })

        it('returns false for a string', () => {
            expect(isIpcError('some error')).toBe(false)
        })

        it('returns false for a number', () => {
            expect(isIpcError(42)).toBe(false)
        })

        it('returns false for a plain Error instance', () => {
            expect(isIpcError(new Error('oops'))).toBe(false)
        })

        it('returns false when message is missing', () => {
            expect(isIpcError({ timedOut: false })).toBe(false)
        })

        it('returns false when timedOut is missing', () => {
            expect(isIpcError({ message: 'err' })).toBe(false)
        })

        it('returns false when message is not a string', () => {
            expect(isIpcError({ message: 123, timedOut: false })).toBe(false)
        })

        it('returns false when timedOut is not a boolean', () => {
            expect(isIpcError({ message: 'err', timedOut: 'yes' })).toBe(false)
        })

        it('returns false for an empty object', () => {
            expect(isIpcError({})).toBe(false)
        })
    })

    // ========================================================================
    // getIpcErrorMessage
    // ========================================================================

    describe('getIpcErrorMessage', () => {
        it('extracts message from an IpcError', () => {
            expect(getIpcErrorMessage({ message: 'backend failure', timedOut: false })).toBe('backend failure')
        })

        it('extracts message from a standard Error', () => {
            expect(getIpcErrorMessage(new Error('standard error'))).toBe('standard error')
        })

        it('converts a string to itself', () => {
            expect(getIpcErrorMessage('raw string')).toBe('raw string')
        })

        it('converts a number to its string representation', () => {
            expect(getIpcErrorMessage(404)).toBe('404')
        })

        it('converts null to string', () => {
            expect(getIpcErrorMessage(null)).toBe('null')
        })

        it('converts undefined to string', () => {
            expect(getIpcErrorMessage(undefined)).toBe('undefined')
        })

        it('prefers IpcError branch over Error branch', () => {
            // An object that satisfies both IpcError shape and has a message property
            const hybrid = { message: 'ipc message', timedOut: true }
            expect(getIpcErrorMessage(hybrid)).toBe('ipc message')
        })
    })
})
