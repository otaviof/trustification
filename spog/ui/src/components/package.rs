use std::{rc::Rc, str::FromStr};

use packageurl::PackageUrl;
use patternfly_yew::{
    next::{
        use_table_data, Cell, CellContext, ColumnWidth, MemoizedTableModel, Table, TableColumn, TableEntryRenderer,
        TableHeader, Toolbar, ToolbarContent, UseTableData,
    },
    prelude::*,
};
use spog_model::prelude::*;
use url::Url;
use yew::prelude::*;
use yew_more_hooks::hooks::{use_async_with_cloned_deps, UseAsyncHandleDeps};
use yew_nested_router::components::Link;

use crate::{
    backend::{Endpoint, PackageService, SearchOptions},
    hooks::use_backend,
    pages::AppRoute,
};

#[derive(PartialEq, Properties)]
pub struct PackageSearchProperties {
    pub callback: Callback<UseAsyncHandleDeps<SearchResult<Rc<Vec<PackageSummary>>>, String>>,

    pub query: Option<String>,

    #[prop_or_default]
    pub toolbar_items: ChildrenWithProps<ToolbarItem>,
}

#[function_component(PackageSearch)]
pub fn package_search(props: &PackageSearchProperties) -> Html {
    let backend = use_backend();

    let service = use_memo(|backend| PackageService::new((**backend).clone()), backend.clone());

    let offset = use_state_eq(|| 0);
    let limit = use_state_eq(|| 10);

    // the active query
    let state = use_state_eq(|| {
        // initialize with the state from history, or with a reasonable default
        props.query.clone().unwrap_or_else(|| {
            gloo_utils::history()
                .state()
                .ok()
                .and_then(|state| state.as_string())
                .unwrap_or_else(String::default)
        })
    });

    let search = {
        let service = service.clone();
        use_async_with_cloned_deps(
            move |(state, offset, limit)| async move {
                service
                    .search_packages(
                        &state,
                        &SearchOptions {
                            offset: Some(offset),
                            limit: Some(limit),
                        },
                    )
                    .await
                    .map(|result| result.map(Rc::new))
                    .map_err(|err| err.to_string())
            },
            ((*state).clone(), *offset, *limit),
        )
    };

    use_effect_with_deps(
        |(callback, search)| {
            callback.emit(search.clone());
        },
        (props.callback.clone(), search.clone()),
    );

    // the current value in the text input field
    let text = use_state_eq(|| (*state).clone());

    let onclear = {
        let text = text.clone();
        Callback::from(move |_| {
            text.set(String::new());
        })
    };
    let onset = {
        let state = state.clone();
        let text = text.clone();
        Callback::from(move |()| {
            state.set((*text).clone());
        })
    };

    use_effect_with_deps(
        |query| {
            // store changes to the state in the current history
            let _ = gloo_utils::history().replace_state(&query.into(), "");
        },
        (*state).clone(),
    );

    // pagination

    let total = search.data().and_then(|d| d.total);
    let onlimit = {
        let limit = limit.clone();
        Callback::from(move |n| {
            limit.set(n);
        })
    };
    let onnavigation = {
        if let Some(total) = total {
            let offset = offset.clone();

            let limit = limit.clone();
            Callback::from(move |nav| {
                let o = match nav {
                    Navigation::First => 0,
                    Navigation::Last => total - *limit,
                    Navigation::Next => *offset + *limit,
                    Navigation::Previous => *offset - *limit,
                    Navigation::Page(n) => *limit * n - 1,
                };
                offset.set(o);
            })
        } else {
            Callback::default()
        }
    };

    // render

    html!(
        <>
            <Toolbar>
                <ToolbarContent>
                    <ToolbarGroup>
                        <ToolbarItem r#type={ToolbarItemType::SearchFilter} width={["600px".to_string()]}>
                            <Form onsubmit={onset.reform(|_|())}>
                                // needed to trigger submit when pressing enter in the search field
                                <input type="submit" hidden=true formmethod="dialog" />
                                <InputGroup>
                                    <TextInputGroup>
                                        <TextInputGroupMain
                                            icon={Icon::Search}
                                            placeholder="Search"
                                            value={(*text).clone()}
                                            oninput={ Callback::from(move |data| text.set(data)) }
                                        />
                                        <TextInputGroupUtilities>
                                            <Button icon={Icon::Times} variant={ButtonVariant::Plain} onclick={onclear} />
                                        </TextInputGroupUtilities>
                                        <Button icon={Icon::ArrowRight} variant={ButtonVariant::Control} onclick={onset.reform(|_|())} />
                                    </TextInputGroup>
                                </InputGroup>
                            </Form>
                        </ToolbarItem>
                    </ToolbarGroup>

                    { for props.toolbar_items.iter() }

                    <ToolbarItem r#type={ToolbarItemType::Pagination}>
                        <Pagination
                            total_entries={total}
                            selected_choice={*limit}
                            offset={*offset}
                            entries_per_page_choices={vec![10, 25, 50]}
                            {onnavigation}
                            {onlimit}
                        >
                        </Pagination>
                    </ToolbarItem>

                </ToolbarContent>
                // <ToolbarContent> { for filters.into_iter() } </ToolbarContent>
            </Toolbar>

        </>
    )
}

#[derive(Debug, Properties)]
pub struct PackageResultProperties {
    pub result: SearchResult<Rc<Vec<PackageSummary>>>,
}

impl PartialEq for PackageResultProperties {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.result, &other.result)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Column {
    Name,
    Supplier,
    Products,
    Description,
    Vulnerabilities,
    Version,
}

#[derive(Clone)]
pub struct PackageEntry {
    url: Option<Url>,
    package: PackageSummary,
}

impl TableEntryRenderer<Column> for PackageEntry {
    fn render_cell(&self, context: &CellContext<'_, Column>) -> Cell {
        match context.column {
            Column::Name => html!(&self.package.name).into(),
            Column::Supplier => html!(&self.package.supplier).into(),
            Column::Products => html!(&self.package.dependents.len()).into(),
            Column::Description => html!(&self.package.description).into(),
            Column::Vulnerabilities => {
                html!(<Link<AppRoute> target={AppRoute::Vulnerability { query: format!("affected:\"{}\"", self.package.purl)}}>{self.package.vulnerabilities.len()}</Link<AppRoute>>).into()
            }
            Column::Version => {
                if let Ok(purl) = PackageUrl::from_str(&self.package.purl) {
                    if let Some(version) = purl.version() {
                        html!(version).into()
                    } else {
                        html!().into()
                    }
                } else {
                    html!().into()
                }
            }
        }
    }

    fn render_details(&self) -> Vec<Span> {
        let html = html!(<PackageDetails package={Rc::new(self.clone())} />);
        vec![Span::max(html)]
    }

    fn is_full_width_details(&self) -> Option<bool> {
        Some(true)
    }
}

#[function_component(PackageResult)]
pub fn package_result(props: &PackageResultProperties) -> Html {
    let backend = use_backend();
    let entries: Vec<PackageEntry> = props
        .result
        .result
        .iter()
        .map(|pkg| {
            let url = backend.join(Endpoint::Api, "/api/v1/package").ok();
            PackageEntry {
                package: pkg.clone(),
                url,
            }
        })
        .collect();

    let (entries, onexpand) = use_table_data(MemoizedTableModel::new(Rc::new(entries)));

    let header = html_nested! {
        <TableHeader<Column>>
            <TableColumn<Column> label="Name" index={Column::Name} width={ColumnWidth::Percent(10)}/>
            <TableColumn<Column> label="Version" index={Column::Version} width={ColumnWidth::Percent(15)}/>
            <TableColumn<Column> label="Supplier" index={Column::Supplier} width={ColumnWidth::Percent(10)}/>
            <TableColumn<Column> label="Products" index={Column::Products} width={ColumnWidth::Percent(10)}/>
            <TableColumn<Column> label="Description" index={Column::Description} width={ColumnWidth::Percent(40)}/>
            <TableColumn<Column> label="Vulnerabilities" index={Column::Vulnerabilities} width={ColumnWidth::Percent(15)}/>
        </TableHeader<Column>>
    };

    html!(
         <Table<Column, UseTableData<Column, MemoizedTableModel<PackageEntry>>>
             mode={TableMode::CompactExpandable}
             {header}
             {entries}
             {onexpand}
         />
    )
}

use yew::prelude::*;

#[derive(Clone, Properties)]
pub struct PackageDetailsProps {
    pub package: Rc<PackageEntry>,
}

impl PartialEq for PackageDetailsProps {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.package, &other.package)
    }
}

#[function_component(PackageDetails)]
pub fn package_details(props: &PackageDetailsProps) -> Html {
    let package = use_memo(|props| props.package.clone(), props.clone());

    let base = &package.url;
    let sboms: Vec<sboms::SbomTableEntry> = package
        .package
        .sboms
        .iter()
        .map(|a| sboms::SbomTableEntry {
            id: a.clone(),
            url: if let Some(url) = base.as_ref() {
                url.join(&format!("?id={}", urlencoding::encode(a))).ok()
            } else {
                None
            },
        })
        .collect::<Vec<sboms::SbomTableEntry>>();
    let sboms = Rc::new(sboms);
    html!(
        <Panel>
            <PanelMain>
            <PanelMainBody>{&package.package.description}</PanelMainBody>
            </PanelMain>
            <PanelFooter>
            <h3>{"Related SBOMs"}</h3>
            <sboms::SbomTable entries={sboms} />
            </PanelFooter>
        </Panel>
    )
}

mod sboms {
    use std::rc::Rc;

    use patternfly_yew::{
        next::{use_table_data, MemoizedTableModel, Table, TableColumn, TableEntryRenderer, TableHeader, UseTableData},
        prelude::*,
    };
    use url::Url;
    use yew::prelude::*;

    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Column {
        Id,
        Download,
    }

    #[derive(Clone, PartialEq)]
    pub struct SbomTableEntry {
        pub id: String,
        pub url: Option<Url>,
    }

    #[derive(Properties, Clone, PartialEq)]
    pub struct SbomTableProperties {
        pub entries: Rc<Vec<SbomTableEntry>>,
    }

    #[function_component(SbomTable)]
    pub fn sbom_table(props: &SbomTableProperties) -> Html {
        let (entries, onexpand) = use_table_data(MemoizedTableModel::new(props.entries.clone()));

        let header = html_nested! {
            <TableHeader<Column>>
                <TableColumn<Column> label="Id" index={Column::Id} />
                <TableColumn<Column> label="Download" index={Column::Download} />
            </TableHeader<Column>>
        };

        html!(
            <Table<Column, UseTableData<Column, MemoizedTableModel<SbomTableEntry>>>
                mode={TableMode::Compact}
                {header}
                {entries}
                {onexpand}
            />
        )
    }

    impl TableEntryRenderer<Column> for SbomTableEntry {
        fn render_cell(&self, context: &patternfly_yew::next::CellContext<'_, Column>) -> patternfly_yew::next::Cell {
            match context.column {
                Column::Id => html!(&self.id).into(),
                Column::Download => {
                    if let Some(url) = &self.url {
                        html!(
                            <a href={url.to_string()}>
                                <Button icon={Icon::Download} variant={ButtonVariant::Plain} />
                            </a>
                        )
                        .into()
                    } else {
                        html!().into()
                    }
                }
            }
        }

        fn render_details(&self) -> Vec<Span> {
            vec![Span::max(html!())]
        }

        fn is_full_width_details(&self) -> Option<bool> {
            Some(false)
        }
    }
}