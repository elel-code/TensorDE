use super::*;

#[test]
fn file_uri_percent_encodes_non_ascii_and_spaces() {
    let uri = tensor_files_core::file_uri_from_path(Path::new("/tmp/Tensor Files Test/数值.txt"));
    assert_eq!(
        uri,
        "file:///tmp/Tensor%20Files%20Test/%E6%95%B0%E5%80%BC.txt"
    );
}

#[cfg(unix)]
#[test]
fn file_uri_preserves_selected_symlink_path() {
    let temp = std::env::temp_dir().join(format!(
        "tensor-files-portal-uri-symlink-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).unwrap();
    let target = temp.join("target.txt");
    let link = temp.join("link.txt");
    std::fs::write(&target, "target").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert_eq!(
        tensor_files_core::file_uri_from_path(&link),
        format!("file://{}", link.to_string_lossy())
    );

    let _ = std::fs::remove_dir_all(&temp);
}

#[test]
fn chooser_args_include_portal_modes_before_start_dir() {
    let args = chooser_args(ChooserArgs {
        start_dir: Some(PathBuf::from("/tmp")),
        directory: true,
        multiple: true,
        save_name: Some("note.txt".to_string()),
        title: Some("Pick a note".to_string()),
        accept_label: Some("Select".to_string()),
        filters: vec!["Images\t*.png;*.jpg".to_string()],
        filter_index: Some(0),
        return_filter: true,
        choices: vec!["encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()],
        return_choices: true,
        parent_window: Some("wayland:1_42".to_string()),
        ..ChooserArgs::default()
    });

    assert_eq!(
        args,
        vec![
            "--chooser",
            "--chooser-directory",
            "--chooser-multiple",
            "--chooser-save",
            "note.txt",
            "--chooser-title",
            "Pick a note",
            "--chooser-accept-label",
            "Select",
            "--chooser-filters",
            "Images\t*.png;*.jpg",
            "--chooser-filter-index",
            "0",
            "--chooser-return-filter",
            "--chooser-choices",
            "encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1",
            "--chooser-return-choices",
            "--chooser-parent-window",
            "wayland:1_42",
            "/tmp"
        ]
    );
}

#[test]
fn empty_portal_parent_window_is_not_forwarded() {
    assert_eq!(
        portal_parent_window(String::new()),
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Empty,
        }
    );
    assert_eq!(portal_title(String::new()), None);
    assert_eq!(
        chooser_args(ChooserArgs {
            parent_window: portal_parent_window(String::new()).handle,
            title: portal_title(String::new()),
            ..ChooserArgs::default()
        }),
        vec!["--chooser"]
    );
}

#[test]
fn portal_parent_window_accepts_wayland_handles() {
    assert_eq!(
        portal_parent_window("wayland:1_42".to_string()),
        ParentWindowDecision {
            handle: Some("wayland:1_42".to_string()),
            status: ParentWindowStatus::Accepted,
        }
    );
    assert_eq!(
        portal_parent_window("wayland:".to_string()),
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::EmptyHandle,
        }
    );
    assert_eq!(
        portal_parent_window("malformed".to_string()),
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::Malformed,
        }
    );
    assert_eq!(
        portal_parent_window("unsupported:window".to_string()),
        ParentWindowDecision {
            handle: None,
            status: ParentWindowStatus::UnsupportedScheme,
        }
    );
}

#[test]
fn portal_request_summary_reports_request_shape() {
    let summary = portal_request_summary(PortalRequestDebug {
        method: "OpenFile",
        handle: "/org/freedesktop/portal/desktop/request/1_42/tensor_files",
        start_dir: Some(Path::new("/home/yk")),
        directory: true,
        multiple: true,
        save_kind: "none",
        save_files: 0,
        portal_filters: 2,
        chooser_filters: 1,
        mime_mapped_filters: 1,
        hidden_filters: 1,
        initial_filter_index: Some(0),
        portal_choices: 1,
        parent_status: ParentWindowStatus::Accepted,
        parent_forwarded: true,
    });

    assert_eq!(
        summary,
        "request method=OpenFile handle=/org/freedesktop/portal/desktop/request/1_42/tensor_files start_dir=/home/yk directory=true multiple=true save_kind=none save_files=0 portal_filters=2 chooser_filters=1 mime_mapped_filters=1 hidden_filters=1 initial_filter=0 portal_choices=1 parent_status=accepted parent_forwarded=true parent_binding=metadata-only parent_binding_reason=parent-token-binding-unavailable native_transient=false"
    );
}

#[test]
fn portal_request_summary_reports_missing_optional_state() {
    let summary = portal_request_summary(PortalRequestDebug {
        method: "SaveFile",
        handle: "/org/freedesktop/portal/desktop/request/1_42/tensor_files",
        start_dir: None,
        directory: false,
        multiple: false,
        save_kind: "file",
        save_files: 0,
        portal_filters: 0,
        chooser_filters: 0,
        mime_mapped_filters: 0,
        hidden_filters: 0,
        initial_filter_index: None,
        portal_choices: 0,
        parent_status: ParentWindowStatus::Empty,
        parent_forwarded: false,
    });

    assert_eq!(
        summary,
        "request method=SaveFile handle=/org/freedesktop/portal/desktop/request/1_42/tensor_files start_dir=<none> directory=false multiple=false save_kind=file save_files=0 portal_filters=0 chooser_filters=0 mime_mapped_filters=0 hidden_filters=0 initial_filter=<none> portal_choices=0 parent_status=empty parent_forwarded=false parent_binding=none parent_binding_reason=no-parent-window native_transient=false"
    );
}

#[test]
fn chooser_lifecycle_summary_reports_selected_metadata() {
    let outcome = Ok(ChooserRun::Selected(ChooserResult {
        paths: vec![PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b.txt")],
        filter_index: Some(1),
        choices: vec![("encoding".to_string(), "utf8".to_string())],
    }));

    assert_eq!(
        chooser_lifecycle_summary("/request", &outcome),
        "chooser_finished handle=/request outcome=selected paths=2 filter=1 choices=1"
    );
}

#[test]
fn chooser_lifecycle_summary_reports_cancellation_reason() {
    let outcome = Ok(ChooserRun::Cancelled(ChooserCancelReason::RequestClose));

    assert_eq!(
        chooser_lifecycle_summary("/request", &outcome),
        "chooser_finished handle=/request outcome=cancelled reason=request-close"
    );
}

#[test]
fn chooser_lifecycle_summary_reports_first_failure_line() {
    let outcome = Err("cannot launch chooser\nextra detail".to_string());

    assert_eq!(
        chooser_lifecycle_summary("/request", &outcome),
        "chooser_finished handle=/request outcome=failed error=cannot launch chooser"
    );
}

#[test]
fn chooser_output_can_report_selected_filter_and_choices_without_breaking_paths() {
    let result = parse_chooser_output(
        "TENSOR_FILES_CHOOSER_FILTER\t1\nTENSOR_FILES_CHOOSER_CHOICE\tencoding\tlatin1\n/tmp/file.txt\n",
    );
    assert_eq!(result.filter_index, Some(1));
    assert_eq!(
        result.choices,
        vec![("encoding".to_string(), "latin1".to_string())]
    );
    assert_eq!(result.paths, vec![PathBuf::from("/tmp/file.txt")]);
}

#[test]
fn chooser_output_preserves_path_whitespace() {
    let result = parse_chooser_output(
        "TENSOR_FILES_CHOOSER_FILTER\t0\n/tmp/ leading.txt\n/tmp/trailing.txt \n",
    );

    assert_eq!(result.filter_index, Some(0));
    assert_eq!(
        result.paths,
        vec![
            PathBuf::from("/tmp/ leading.txt"),
            PathBuf::from("/tmp/trailing.txt "),
        ]
    );
}

#[test]
fn portal_filters_map_to_chooser_specs_and_result_filter() {
    let filters = vec![
        (
            "Text".to_string(),
            vec![(0, "*.txt".to_string()), (1, "text/plain".to_string())],
        ),
        ("Images".to_string(), vec![(0, "*.png".to_string())]),
    ];
    let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());
    assert_eq!(
        filter_map.chooser_specs,
        vec![
            "Text\t*.txt;*.text;*.md;*.markdown;*.rst;*.log;*.ini;*.conf".to_string(),
            "Images\t*.png".to_string()
        ]
    );
    assert_eq!(filter_map.mime_mapped_filters, 1);
    assert_eq!(filter_map.hidden_filters, 0);

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.png")],
            filter_index: Some(1),
            choices: Vec::new(),
        },
        &HashMap::new(),
        &filter_map,
    );
    let current_filter = result.get("current_filter").cloned().unwrap();
    assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);
}

#[test]
fn portal_mime_only_filters_map_to_chooser_specs() {
    let filters = vec![
        ("MIME only".to_string(), vec![(1, "image/png".to_string())]),
        ("Images".to_string(), vec![(0, "*.png".to_string())]),
    ];
    let mut options = HashMap::new();
    options.insert(
        "current_filter".to_string(),
        OwnedValue::try_from(Value::new(filters[0].clone())).unwrap(),
    );

    let filter_map = chooser_filter_map(&options, filters.clone());

    assert_eq!(
        filter_map.chooser_specs,
        vec!["MIME only\t*.png".to_string(), "Images\t*.png".to_string()]
    );
    assert_eq!(filter_map.portal_indices, vec![0, 1]);
    assert_eq!(filter_map.initial_chooser_index, Some(0));
    assert_eq!(filter_map.mime_mapped_filters, 1);
    assert_eq!(filter_map.hidden_filters, 0);

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.png")],
            filter_index: Some(0),
            choices: Vec::new(),
        },
        &options,
        &filter_map,
    );
    let current_filter = result.get("current_filter").cloned().unwrap();
    assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[0]);
}

#[test]
fn portal_filters_with_empty_labels_get_stable_chooser_labels() {
    let filters = vec![
        ("".to_string(), vec![(1, "image/png".to_string())]),
        ("  ".to_string(), vec![(0, "*.txt".to_string())]),
    ];
    let mut options = HashMap::new();
    options.insert(
        "current_filter".to_string(),
        OwnedValue::try_from(Value::new(filters[1].clone())).unwrap(),
    );

    let filter_map = chooser_filter_map(&options, filters.clone());

    assert_eq!(
        filter_map.chooser_specs,
        vec!["Filter 1\t*.png".to_string(), "Filter 2\t*.txt".to_string()]
    );
    assert_eq!(filter_map.portal_indices, vec![0, 1]);
    assert_eq!(filter_map.initial_chooser_index, Some(1));

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            filter_index: Some(1),
            choices: Vec::new(),
        },
        &options,
        &filter_map,
    );
    let current_filter = result.get("current_filter").cloned().unwrap();
    assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);
}

#[test]
fn portal_unknown_mime_only_filters_stay_hidden() {
    let filters = vec![
        (
            "Unknown".to_string(),
            vec![(1, "application/x-tensor-files-unknown".to_string())],
        ),
        (
            "Any text".to_string(),
            vec![(1, "text/*".to_string()), (1, "text/plain".to_string())],
        ),
    ];

    let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());

    assert_eq!(
        filter_map.chooser_specs,
        vec![
            "Any text\t*.txt;*.text;*.md;*.markdown;*.rst;*.csv;*.tsv;*.log;*.ini;*.conf;*.toml;*.json;*.yaml;*.yml;*.xml;*.html;*.htm;*.css;*.js;*.rs;*.c;*.h;*.cpp;*.hpp;*.py;*.sh"
                .to_string()
        ]
    );
    assert_eq!(filter_map.portal_indices, vec![1]);
    assert_eq!(filter_map.mime_mapped_filters, 1);
    assert_eq!(filter_map.hidden_filters, 1);
}

#[test]
fn portal_media_and_document_mime_filters_map_to_chooser_specs() {
    let filters = vec![
        ("Audio".to_string(), vec![(1, "audio/*".to_string())]),
        ("Video".to_string(), vec![(1, "video/mp4".to_string())]),
        (
            "AVIF".to_string(),
            vec![(1, "image/avif-sequence".to_string())],
        ),
        (
            "Documents".to_string(),
            vec![
                (
                    1,
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .to_string(),
                ),
                (1, "application/vnd.oasis.opendocument.text".to_string()),
            ],
        ),
        (
            "Sheets".to_string(),
            vec![
                (
                    1,
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
                ),
                (1, "application/vnd.ms-excel".to_string()),
            ],
        ),
    ];

    let filter_map = chooser_filter_map(&HashMap::new(), filters);

    assert_eq!(
        filter_map.chooser_specs,
        vec![
            "Audio\t*.aac;*.flac;*.m4a;*.mp3;*.oga;*.ogg;*.opus;*.wav".to_string(),
            "Video\t*.mp4;*.m4v".to_string(),
            "AVIF\t*.avif;*.avifs".to_string(),
            "Documents\t*.docx;*.odt".to_string(),
            "Sheets\t*.xlsx;*.xls".to_string(),
        ]
    );
    assert_eq!(filter_map.portal_indices, vec![0, 1, 2, 3, 4]);
    assert_eq!(filter_map.mime_mapped_filters, 5);
    assert_eq!(filter_map.hidden_filters, 0);
}

#[test]
fn portal_result_filter_maps_supported_chooser_indices() {
    let filters = vec![
        ("MIME only".to_string(), vec![(1, "image/png".to_string())]),
        (
            "Text".to_string(),
            vec![(0, "*.txt".to_string()), (1, "text/plain".to_string())],
        ),
        ("Images".to_string(), vec![(0, "*.png".to_string())]),
    ];
    let filter_map = chooser_filter_map(&HashMap::new(), filters.clone());
    assert_eq!(
        filter_map.chooser_specs,
        vec![
            "MIME only\t*.png".to_string(),
            "Text\t*.txt;*.text;*.md;*.markdown;*.rst;*.log;*.ini;*.conf".to_string(),
            "Images\t*.png".to_string()
        ]
    );
    assert_eq!(filter_map.portal_indices, vec![0, 1, 2]);
    assert_eq!(filter_map.mime_mapped_filters, 2);
    assert_eq!(filter_map.hidden_filters, 0);

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            filter_index: Some(1),
            choices: Vec::new(),
        },
        &HashMap::new(),
        &filter_map,
    );
    let current_filter = result.get("current_filter").cloned().unwrap();
    assert_eq!(PortalFilter::try_from(current_filter).unwrap(), filters[1]);

    let out_of_range = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            filter_index: Some(99),
            choices: Vec::new(),
        },
        &HashMap::new(),
        &filter_map,
    );
    assert!(!out_of_range.contains_key("current_filter"));
}

#[test]
fn portal_choices_map_to_specs_and_result_choices() {
    let choices: Vec<PortalChoice> = vec![(
        "encoding".to_string(),
        "Encoding".to_string(),
        vec![
            ("utf8".to_string(), "UTF-8".to_string()),
            ("latin1".to_string(), "Latin-1".to_string()),
        ],
        "utf8".to_string(),
    )];
    assert_eq!(
        chooser_choice_specs(&choices),
        vec!["encoding\tEncoding\tutf8\tutf8=UTF-8;latin1=Latin-1".to_string()]
    );

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            filter_index: None,
            choices: vec![("encoding".to_string(), "latin1".to_string())],
        },
        &options_with_choices(choices.clone()),
        &ChooserFilterMap::default(),
    );
    let selected =
        Vec::<(String, String)>::try_from(result.get("choices").cloned().unwrap()).unwrap();
    assert_eq!(
        selected,
        vec![("encoding".to_string(), "latin1".to_string())]
    );
}

#[test]
fn portal_choice_specs_sanitize_labels_and_drop_unsafe_ids() {
    let choices: Vec<PortalChoice> = vec![
        (
            "encoding".to_string(),
            "Encoding\tMode=Fast;Safe".to_string(),
            vec![
                ("utf8".to_string(), "UTF-8\nDefault".to_string()),
                ("latin;1".to_string(), "Latin-1".to_string()),
                ("raw".to_string(), "Raw=Bytes;Exact".to_string()),
            ],
            "missing-default".to_string(),
        ),
        (
            "bad\tchoice".to_string(),
            "Broken".to_string(),
            vec![("ok".to_string(), "OK".to_string())],
            "ok".to_string(),
        ),
        (
            "empty-items".to_string(),
            "Empty Items".to_string(),
            vec![("bad=option".to_string(), "Bad".to_string())],
            "bad=option".to_string(),
        ),
    ];

    assert_eq!(
        chooser_choice_specs(&choices),
        vec!["encoding\tEncoding Mode Fast Safe\t\tutf8=UTF-8 Default;raw=Raw Bytes Exact"]
    );
}

#[test]
fn portal_choice_specs_keep_original_safe_ids_for_result_mapping() {
    let choices: Vec<PortalChoice> = vec![(
        "output mode".to_string(),
        "Output Mode".to_string(),
        vec![("plain text".to_string(), "Plain Text".to_string())],
        "plain text".to_string(),
    )];

    assert_eq!(
        chooser_choice_specs(&choices),
        vec!["output mode\tOutput Mode\tplain text\tplain text=Plain Text"]
    );

    let result = results_for_paths(
        ChooserResult {
            paths: vec![PathBuf::from("/tmp/a.txt")],
            filter_index: None,
            choices: vec![("output mode".to_string(), "plain text".to_string())],
        },
        &options_with_choices(choices),
        &ChooserFilterMap::default(),
    );
    let selected =
        Vec::<(String, String)>::try_from(result.get("choices").cloned().unwrap()).unwrap();
    assert_eq!(
        selected,
        vec![("output mode".to_string(), "plain text".to_string())]
    );
}

#[test]
fn portal_byte_arrays_decode_to_paths_and_file_names() {
    let mut options = HashMap::new();
    options.insert(
        "current_folder".to_string(),
        OwnedValue::try_from(Value::new(b"/tmp/tensor-files\0".to_vec())).unwrap(),
    );
    options.insert(
        "files".to_string(),
        OwnedValue::try_from(Value::new(vec![
            b"one.txt\0".to_vec(),
            b"two.txt\0".to_vec(),
        ]))
        .unwrap(),
    );

    assert_eq!(
        current_folder(&options),
        Some(PathBuf::from("/tmp/tensor-files"))
    );
    assert_eq!(save_files(&options), vec!["one.txt", "two.txt"]);
}

#[test]
fn portal_byte_arrays_decode_network_locations() {
    let mut options = HashMap::new();
    options.insert(
        "current_folder".to_string(),
        OwnedValue::try_from(Value::new(b"smb://server/share/\0".to_vec())).unwrap(),
    );
    options.insert(
        "current_file".to_string(),
        OwnedValue::try_from(Value::new(b"smb://server/share/report.txt\0".to_vec())).unwrap(),
    );

    assert_eq!(
        current_folder(&options),
        Some(PathBuf::from("smb://server/share/"))
    );
    assert_eq!(
        save_file_start_and_name(&options),
        (
            Some(PathBuf::from("smb://server/share/")),
            Some("report.txt".to_string())
        )
    );
}

#[test]
fn portal_results_preserve_network_uris() {
    let result = results_for_paths(
        ChooserResult {
            paths: vec![
                PathBuf::from("smb://server/share/report.txt"),
                PathBuf::from("/tmp/local.txt"),
            ],
            filter_index: None,
            choices: Vec::new(),
        },
        &HashMap::new(),
        &ChooserFilterMap::default(),
    );
    let uris = Vec::<String>::try_from(result.get("uris").cloned().unwrap()).unwrap();

    assert_eq!(
        uris,
        vec![
            "smb://server/share/report.txt".to_string(),
            "file:///tmp/local.txt".to_string(),
        ]
    );
}

#[test]
fn portal_choices_return_default_selection() {
    let choices: Vec<PortalChoice> = vec![(
        "encoding".to_string(),
        "Encoding".to_string(),
        vec![("utf8".to_string(), "UTF-8".to_string())],
        "utf8".to_string(),
    )];

    assert_eq!(
        selected_choices_for_options(&options_with_choices(choices), &[]),
        Some(vec![("encoding".to_string(), "utf8".to_string())])
    );
}

#[test]
fn portal_choices_ignore_unknown_or_invalid_chooser_output() {
    let choices: Vec<PortalChoice> = vec![
        (
            "encoding".to_string(),
            "Encoding".to_string(),
            vec![
                ("utf8".to_string(), "UTF-8".to_string()),
                ("latin1".to_string(), "Latin-1".to_string()),
            ],
            "utf8".to_string(),
        ),
        (
            "mode".to_string(),
            "Mode".to_string(),
            vec![("read".to_string(), "Read".to_string())],
            "missing-default".to_string(),
        ),
    ];

    assert_eq!(
        selected_choices_for_options(
            &options_with_choices(choices),
            &[
                ("encoding".to_string(), "unknown-option".to_string()),
                ("unknown-choice".to_string(), "latin1".to_string()),
            ],
        ),
        Some(vec![
            ("encoding".to_string(), "utf8".to_string()),
            ("mode".to_string(), "read".to_string())
        ])
    );
}

fn options_with_choices(choices: Vec<PortalChoice>) -> HashMap<String, OwnedValue> {
    let mut options = HashMap::new();
    options.insert(
        "choices".to_string(),
        OwnedValue::try_from(Value::new(choices)).unwrap(),
    );
    options
}

#[test]
fn chooser_failure_message_includes_exit_status_and_stderr() {
    assert_eq!(
        chooser_failure_message(Some(2), "bad filter"),
        "tensor-files chooser failed with exit code 2: bad filter"
    );
    assert_eq!(
        chooser_failure_message(Some(3), ""),
        "tensor-files chooser failed with exit code 3"
    );
    assert_eq!(
        chooser_failure_message(None, "killed"),
        "tensor-files chooser failed with terminated by signal: killed"
    );
}

#[test]
fn chooser_process_keeps_kill_on_drop_as_lifecycle_fallback() {
    let command = chooser_command(PathBuf::from("/bin/true"), vec!["--chooser".to_string()]);
    assert!(chooser_command_kills_on_drop(&command));
}

#[test]
fn chooser_process_is_explicitly_terminated_for_request_close() {
    let process = ChooserProcess::spawn(chooser_command(
        PathBuf::from("/bin/sleep"),
        vec!["30".into()],
    ))
    .unwrap();

    let output = futures_lite::future::block_on(async {
        use futures_lite::future::FutureExt;
        process
            .terminate_for_test()
            .or(async {
                async_io::Timer::after(std::time::Duration::from_secs(2)).await;
                Err("chooser termination timed out waiting for process exit".to_string())
            })
            .await
    })
    .expect("chooser termination should return process output");

    assert!(!output.status.success());
}

#[test]
fn portal_request_close_match_targets_exact_request_handle() {
    let rule =
        request_close_match_rule("/org/freedesktop/portal/desktop/request/1_42/tensor_files")
            .unwrap();

    assert_eq!(
        rule.to_string(),
        "type='signal',interface='org.freedesktop.impl.portal.Request',member='Close',path='/org/freedesktop/portal/desktop/request/1_42/tensor_files'"
    );
}
