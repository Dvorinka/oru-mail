import React from 'react';
import fs from 'fs';
import classNames from 'classnames';

import { RetinaImg } from 'mailspring-component-kit';
import { Actions, PreferencesUIStoreTab, Utils, localized } from 'mailspring-exports';

interface PreferencesTabItemProps {
  selection: {
    accountId: string;
    tabId: string;
  };
  tabItem?: PreferencesUIStoreTab;
}

class PreferencesTabItem extends React.Component<PreferencesTabItemProps> {
  static displayName = 'PreferencesTabItem';

  _onClick = () => {
    Actions.switchPreferencesTab(this.props.tabItem.tabId);
  };

  _onClickAccount = (event: React.MouseEvent, accountId: string) => {
    Actions.switchPreferencesTab(this.props.tabItem.tabId, { accountId });
    event.stopPropagation();
  };

  _onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      this._onClick();
    }
  };

  render() {
    const { selection, tabItem } = this.props;
    const { tabId, displayName } = tabItem;
    const classes = classNames({
      item: true,
      active: tabId === selection.tabId,
    });

    const icon = (
      <RetinaImg
        style={{ width: 40, height: 40 }}
        className="tab-icon"
        name={`icon-preferences-${tabId.toLowerCase().replace(' ', '-')}.png`}
        fallback={`icon-preferences-general.png`}
        mode={RetinaImg.Mode.ContentPreserve}
      />
    );

    const isSelected = tabId === selection.tabId;
    return (
      <div
        className={classes}
        role="tab"
        aria-selected={isSelected}
        aria-label={displayName}
        tabIndex={isSelected ? 0 : -1}
        onClick={this._onClick}
        onKeyDown={this._onKeyDown}
      >
        {icon}
        <div className="name">{displayName}</div>
      </div>
    );
  }
}

interface PreferencesTabBarProps {
  tabs: PreferencesUIStoreTab[];
  selection: {
    accountId: string;
    tabId: string;
  };
  query: string;
}

class PreferencesTabsBar extends React.Component<PreferencesTabBarProps> {
  static displayName = 'PreferencesTabsBar';

  private _listRef = React.createRef<HTMLDivElement>();

  _visibleTabs() {
    const query = (this.props.query || '').trim().toLowerCase();
    if (!query) {
      return this.props.tabs;
    }
    return this.props.tabs.filter((tab) => tab.displayName.toLowerCase().includes(query));
  }

  _onKeyDown = (e: React.KeyboardEvent) => {
    const tabs = this._visibleTabs();
    if (tabs.length === 0) {
      return;
    }
    const { selection } = this.props;
    const currentIdx = tabs.findIndex((t) => t.tabId === selection.tabId);

    let nextIdx: number | null = null;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
      nextIdx = currentIdx < 0 ? 0 : (currentIdx + 1) % tabs.length;
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
      nextIdx = currentIdx < 0 ? tabs.length - 1 : (currentIdx - 1 + tabs.length) % tabs.length;
    } else if (e.key === 'Home') {
      nextIdx = 0;
    } else if (e.key === 'End') {
      nextIdx = tabs.length - 1;
    } else {
      return;
    }

    e.preventDefault();
    Actions.switchPreferencesTab(tabs[nextIdx].tabId);

    // Focus the newly activated tab (works even before React re-renders tabIndex)
    const tabEls = this._listRef.current?.querySelectorAll<HTMLElement>('[role="tab"]');
    tabEls?.[nextIdx]?.focus();
  };

  renderTabs() {
    return this._visibleTabs().map((tabItem) => (
      <PreferencesTabItem key={tabItem.tabId} tabItem={tabItem} selection={this.props.selection} />
    ));
  }

  render() {
    return (
      <div className="container-preference-tabs">
        <div
          ref={this._listRef}
          className="preferences-tabs"
          role="tablist"
          aria-label={localized('Preferences tabs')}
          onKeyDown={this._onKeyDown}
        >
          <div style={{ flex: 0.5 }} />
          {this.renderTabs()}
          <div style={{ flex: 0.5 }} />
        </div>
      </div>
    );
  }
}

export default PreferencesTabsBar;
