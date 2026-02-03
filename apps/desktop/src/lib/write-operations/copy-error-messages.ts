/**
 * User-friendly error message generation for copy operations.
 * Extracted from CopyErrorDialog.svelte for testability.
 */

import type { WriteOperationError } from '$lib/file-explorer/types'
import { formatBytes } from '$lib/tauri-commands'
import { getDevice } from '$lib/mtp/mtp-store.svelte'

export interface FriendlyErrorMessage {
    /** Short title for the error */
    title: string
    /** Main explanation of what happened */
    message: string
    /** Suggestion for what the user can do */
    suggestion: string
}

/**
 * Returns a user-friendly message for a copy operation error.
 * Volume-agnostic: doesn't mention MTP, SMB, etc. directly.
 */
export function getUserFriendlyMessage(error: WriteOperationError): FriendlyErrorMessage {
    switch (error.type) {
        case 'source_not_found':
            return {
                title: "Couldn't find the file",
                message: 'The file or folder you tried to copy no longer exists.',
                suggestion: 'It may have been moved, renamed, or deleted. Try refreshing the file list.',
            }
        case 'destination_exists':
            return {
                title: 'File already exists',
                message: "There's already a file with this name at the destination.",
                suggestion: 'Choose a different name or location, or delete the existing file first.',
            }
        case 'permission_denied':
            return {
                title: "Couldn't access this location",
                message: "You don't have permission to copy files here.",
                suggestion:
                    'Check that you have write access to the destination folder. You may need to unlock the device or change folder permissions.',
            }
        case 'insufficient_space':
            return {
                title: 'Not enough space',
                message: `The destination needs ${formatBytes(error.required)} but only has ${formatBytes(error.available)} available.`,
                suggestion:
                    'Free up some space on the destination by deleting unnecessary files, or choose a different location.',
            }
        case 'same_location':
            return {
                title: "Can't copy to the same location",
                message: 'The source and destination are the same.',
                suggestion: 'Choose a different destination folder.',
            }
        case 'destination_inside_source':
            return {
                title: "Can't copy a folder into itself",
                message: "You're trying to copy a folder into one of its own subfolders.",
                suggestion: 'Choose a destination outside of the folder you are copying.',
            }
        case 'symlink_loop':
            return {
                title: 'Link loop detected',
                message: 'This folder contains symbolic links that create an infinite loop.',
                suggestion:
                    'The folder structure contains circular references. You may need to remove some symbolic links.',
            }
        case 'cancelled':
            return {
                title: 'Copy cancelled',
                message: 'The copy operation was cancelled.',
                suggestion: 'You can try again when ready.',
            }
        case 'io_error':
            return {
                title: 'Copy failed',
                message: getIoErrorMessage(error.message),
                suggestion: getIoErrorSuggestion(error.message),
            }
        default:
            return {
                title: 'Copy failed',
                message: 'An unexpected error occurred while copying.',
                suggestion: 'Try again, or check the technical details below for more information.',
            }
    }
}

/**
 * Extracts a friendly device name from an MTP device ID in an error message.
 * Falls back to "The target device" if device not found.
 */
function getDeviceNameFromError(rawMessage: string): string {
    // Extract device ID pattern like "mtp-35651584" from the message
    const deviceIdMatch = rawMessage.match(/mtp-\d+/)
    if (deviceIdMatch) {
        const deviceId = deviceIdMatch[0]
        const device = getDevice(deviceId)
        if (device) {
            return device.displayName
        }
    }
    return 'The target device'
}

/**
 * Parses IO error messages into user-friendly text.
 */
function getIoErrorMessage(rawMessage: string): string {
    const lower = rawMessage.toLowerCase()

    // Read-only device (check BEFORE generic "read" + "error" check!)
    if (lower.includes('read-only')) {
        const deviceName = getDeviceNameFromError(rawMessage)
        return `${deviceName} is read-only. You can copy files from it, but not to it.`
    }

    // Device disconnected
    if (lower.includes('disconnect') || lower.includes('not found') || lower.includes('no such device')) {
        return 'The device was disconnected during the copy.'
    }

    // Connection errors
    if (lower.includes('connection') || lower.includes('timeout') || lower.includes('timed out')) {
        return 'The connection was interrupted.'
    }

    // Read/write errors
    if (lower.includes('read') && lower.includes('error')) {
        return "Couldn't read from the source."
    }
    if (lower.includes('write') && lower.includes('error')) {
        return "Couldn't write to the destination."
    }

    // File system errors
    if (lower.includes('name too long')) {
        return 'The file name is too long for the destination.'
    }
    if (lower.includes('invalid') && lower.includes('name')) {
        return 'The file name contains characters not allowed at the destination.'
    }

    // Default
    return 'An error occurred while copying the file.'
}

/**
 * Returns a helpful suggestion based on the IO error.
 */
function getIoErrorSuggestion(rawMessage: string): string {
    const lower = rawMessage.toLowerCase()

    // Read-only device - no action the user can take
    if (lower.includes('read-only')) {
        return 'Choose a different destination that supports writing.'
    }

    if (lower.includes('disconnect') || lower.includes('not found') || lower.includes('no such device')) {
        return 'Make sure the device is properly connected and try again.'
    }

    if (lower.includes('connection') || lower.includes('timeout') || lower.includes('timed out')) {
        return 'Check your connection and try again. If copying to a network location, ensure the server is reachable.'
    }

    if (lower.includes('name too long') || (lower.includes('invalid') && lower.includes('name'))) {
        return 'Try renaming the file to use a shorter name or remove special characters.'
    }

    return 'Try again. If the problem persists, check the technical details below.'
}

/**
 * Returns the technical details for an error (path, raw error message, etc.)
 */
export function getTechnicalDetails(error: WriteOperationError): string {
    const lines: string[] = []

    switch (error.type) {
        case 'source_not_found':
        case 'destination_exists':
        case 'same_location':
        case 'symlink_loop':
            lines.push(`Path: ${error.path}`)
            break
        case 'permission_denied':
            lines.push(`Path: ${error.path}`)
            if (error.message) {
                lines.push(`Details: ${error.message}`)
            }
            break
        case 'insufficient_space':
            lines.push(`Required: ${formatBytes(error.required)}`)
            lines.push(`Available: ${formatBytes(error.available)}`)
            if (error.volumeName) {
                lines.push(`Volume: ${error.volumeName}`)
            }
            break
        case 'destination_inside_source':
            lines.push(`Source: ${error.source}`)
            lines.push(`Destination: ${error.destination}`)
            break
        case 'cancelled':
            if (error.message) {
                lines.push(`Details: ${error.message}`)
            }
            break
        case 'io_error':
            lines.push(`Path: ${error.path}`)
            lines.push(`Error: ${error.message}`)
            break
    }

    lines.push(`Error type: ${error.type}`)

    return lines.join('\n')
}
