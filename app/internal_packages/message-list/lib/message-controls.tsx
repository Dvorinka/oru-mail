/* eslint global-require: 0 */

import React from 'react';
import {
  localized,
  Actions,
  TaskQueue,
  TaskFactory,
  GetMessageRFC2822Task,
  EmlUtils,
  Thread,
  Message,
  ComponentRegistry,
} from 'mailspring-exports';
import { RetinaImg, ButtonDropdown, Menu } from 'mailspring-component-kit';
import { developerToolsAvailable } from '../../../src/developer-tools';

export const BLOCKED_SENDERS_KEY = 'core.blockedSenders';

export function blockedSenderEmails(): string[] {
  return (AppEnv.config.get(BLOCKED_SENDERS_KEY) as string[]) || [];
}

interface MessageControlsProps {
  thread: Thread;
  message: Message;
}

export default class MessageControls extends React.Component<
  MessageControlsProps,
  { blocked: boolean }
> {
  static displayName = 'MessageControls';

  state = {
    blocked: this._senderIsBlocked(),
  };

  componentDidMount() {
    this._configSub = AppEnv.config.onDidChange(BLOCKED_SENDERS_KEY, () => {
      this.setState({ blocked: this._senderIsBlocked() });
    });
  }

  componentWillUnmount() {
    if (this._configSub) {
      this._configSub.dispose();
    }
  }

  _configSub: { dispose: () => void } | null = null;

  _senderEmail() {
    const from = this.props.message && this.props.message.from;
    return from && from[0] ? (from[0].email || '').toLowerCase() : '';
  }

  _senderIsBlocked() {
    const email = this._senderEmail();
    return !!email && blockedSenderEmails().includes(email);
  }

  _onToggleBlock = () => {
    const email = this._senderEmail();
    if (!email) {
      return;
    }
    const list = blockedSenderEmails();
    const blocked = list.includes(email);
    AppEnv.config.set(
      BLOCKED_SENDERS_KEY,
      blocked ? list.filter((e) => e !== email) : [...list, email]
    );
    this.setState({ blocked: !blocked });

    if (!blocked && this.props.thread) {
      const tasks = TaskFactory.tasksForMovingToTrash({
        threads: [this.props.thread],
        source: 'Message Header: Block Sender',
      });
      Actions.queueTasks(tasks);
    }
  };

  _renderBlockBtn() {
    const { message } = this.props;
    if (!message || message.draft || (message.isFromMe && message.isFromMe())) {
      return null;
    }
    return (
      <div
        className="blockBtn"
        role="button"
        tabIndex={0}
        onClick={this._onToggleBlock}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            this._onToggleBlock();
          }
        }}
      >
        {this.state.blocked ? localized('Unblock') : localized('Block')}
      </div>
    );
  }

  _items() {
    const reply = {
      name: localized('Reply'),
      image: 'ic-dropdown-reply.png',
      select: this._onReply,
    };
    const replyAll = {
      name: localized('Reply All'),
      image: 'ic-dropdown-replyall.png',
      select: this._onReplyAll,
    };
    const forward = {
      name: localized('Forward'),
      image: 'ic-dropdown-forward.png',
      select: this._onForward,
    };

    const showOriginal = {
      name: localized('Show Original'),
      image: 'ic-dropdown-whitespace.png',
      select: this._onShowOriginal,
    };

    if (!this.props.message.canReplyAll()) {
      return [reply, forward, showOriginal];
    }
    const defaultReplyType = AppEnv.config.get('core.sending.defaultReplyType');
    return defaultReplyType === 'reply-all'
      ? [replyAll, reply, forward, showOriginal]
      : [reply, replyAll, forward, showOriginal];
  }

  _dropdownMenu(items: Array<{ name: string; image: string; select: () => void }>) {
    const itemContent = (item: { name: string; image: string; select: () => void }) => (
      <span>
        <RetinaImg name={item.image} mode={RetinaImg.Mode.ContentIsMask} />
        &nbsp;&nbsp;{item.name}&nbsp;&nbsp;
      </span>
    );

    return (
      <Menu
        items={items}
        itemKey={(item) => item.name}
        itemContent={itemContent}
        onSelect={(item) => item.select()}
      />
    );
  }

  _onReply = () => {
    const { thread, message } = this.props;
    Actions.composeReply({
      thread,
      message,
      type: 'reply',
      behavior: 'prefer-existing-if-pristine',
    });
  };

  _onReplyAll = () => {
    const { thread, message } = this.props;
    Actions.composeReply({
      thread,
      message,
      type: 'reply-all',
      behavior: 'prefer-existing-if-pristine',
    });
  };

  _onForward = () => {
    const { thread, message } = this.props;
    Actions.composeForward({ thread, message });
  };

  _onDownloadEml = () => {
    const { message } = this.props;
    const defaultFilename = EmlUtils.defaultEmlFilename(message.subject);

    AppEnv.showSaveDialog(
      { defaultPath: defaultFilename, title: localized('Save Email') },
      async (savePath: string) => {
        if (!savePath) return;
        const task = new GetMessageRFC2822Task({
          messageId: message.id,
          accountId: message.accountId,
          filepath: savePath,
        });
        Actions.queueTask(task);
        await TaskQueue.waitForPerformRemote(task);
      }
    );
  };

  _onShowActionsMenu = () => {
    const SystemMenu = require('@electron/remote').Menu;
    const SystemMenuItem = require('@electron/remote').MenuItem;

    // Todo: refactor this so that message actions are provided
    // dynamically. Waiting to see if this will be used often.
    const menu = new SystemMenu();
    menu.append(new SystemMenuItem({ label: localized('Log Data'), click: this._onLogData }));
    menu.append(
      new SystemMenuItem({ label: localized('Show Original'), click: this._onShowOriginal })
    );
    menu.append(
      new SystemMenuItem({
        label: localized('Copy Debug Info to Clipboard'),
        click: this._onCopyToClipboard,
      })
    );
    menu.append(new SystemMenuItem({ type: 'separator' }));
    menu.append(
      new SystemMenuItem({ label: localized('Download as .eml'), click: this._onDownloadEml })
    );

    const actionItems = ComponentRegistry.findComponentsMatching({
      role: 'MessageActionMenuItem',
    }) as Array<{
      getMenuItem: (props: MessageControlsProps) => { label: string; click: () => void };
    }>;
    if (actionItems.length > 0) {
      menu.append(new SystemMenuItem({ type: 'separator' }));
      for (const ActionItem of actionItems) {
        const item = ActionItem.getMenuItem(this.props);
        menu.append(new SystemMenuItem({ label: item.label, click: item.click }));
      }
    }

    menu.popup({});
  };

  _onShowOriginal = async () => {
    const { message } = this.props;
    const filepath = require('path').join(
      require('@electron/remote').app.getPath('temp'),
      message.id
    );
    const task = new GetMessageRFC2822Task({
      messageId: message.id,
      accountId: message.accountId,
      filepath,
    });
    Actions.queueTask(task);
    await TaskQueue.waitForPerformRemote(task);

    // The task can reach "complete" status even when it failed remotely (eg.
    // the message could not be fetched from the server), in which case the
    // file is never written. Verify it exists before trying to display it.
    if (!require('fs').existsSync(filepath)) {
      AppEnv.showErrorDialog(
        localized('Could not retrieve the original message. Please try again.')
      );
      return;
    }

    const { BrowserWindow } = require('@electron/remote');
    const win = new BrowserWindow({
      width: 800,
      height: 600,
      title: `${message.subject} - RFC822`,
      webPreferences: {
        javascript: false,
        nodeIntegration: false,
        devTools: developerToolsAvailable(),
      },
    });
    win.loadURL(`file://${filepath}`).catch((err: Error) => {
      console.error('Show Original window failed to load:', err);
    });
  };

  _onLogData = () => {
    console.log(this.props.message);
    (window as any).__message = this.props.message;
    (window as any).__thread = this.props.thread;
    console.log('Also now available in window.__message and window.__thread');
  };

  _onCopyToClipboard = () => {
    const { message, thread } = this.props;
    const data = `
      AccountID: ${message.accountId}
      Message ID: ${message.id}
      Message Metadata: ${JSON.stringify(message.pluginMetadata, null, '  ')}
      Thread ID: ${thread.id}
      Thread Metadata: ${JSON.stringify(thread.pluginMetadata, null, '  ')}
    `;
    require('@electron/remote').clipboard.writeText(data);
  };

  render() {
    const items = this._items();
    return (
      <div className="message-actions-wrap" onClick={(e) => e.stopPropagation()}>
        <ButtonDropdown
          primaryItem={<RetinaImg name={items[0].image} mode={RetinaImg.Mode.ContentIsMask} />}
          primaryTitle={items[0].name}
          primaryClick={items[0].select}
          closeOnMenuClick
          menu={this._dropdownMenu(items.slice(1))}
        />
        {this._renderBlockBtn()}
        <div
          className="message-actions-ellipsis"
          tabIndex={-1}
          onClick={this._onShowActionsMenu}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              this._onShowActionsMenu();
            }
          }}
        >
          <RetinaImg
            name={'message-actions-ellipsis.png'}
            mode={RetinaImg.Mode.ContentIsMask}
            aria-hidden="true"
          />
        </div>
      </div>
    );
  }
}
