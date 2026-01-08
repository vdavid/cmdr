# SMB server with extremely long share/path names
# Useful for testing path truncation and breadcrumb overflow
FROM alpine:3.21

RUN apk add --no-cache samba samba-client

# Create share with 200+ character name
RUN LONG_NAME="this_is_a_very_long_share_name_that_exceeds_two_hundred_characters_and_is_designed_to_test_how_the_ui_handles_extremely_long_share_names_in_the_volume_selector_and_breadcrumb_navigation" && \
    mkdir -p "/shares/${LONG_NAME}" && \
    echo "Long name content" > "/shares/${LONG_NAME}/readme.txt" && \
    mkdir -p "/shares/normal_share" && \
    # Create a deeply nested long path
    NESTED_PATH="/shares/normal_share/$(printf 'a%.0s' {1..50})/$(printf 'b%.0s' {1..50})/$(printf 'c%.0s' {1..50})" && \
    mkdir -p "$NESTED_PATH" && \
    echo "Deep nested long path content" > "$NESTED_PATH/deep.txt"

COPY smb.conf /etc/samba/smb.conf

EXPOSE 445

CMD ["smbd", "--foreground", "--no-process-group", "--debug-stdout"]
