import React from 'react';
import { localized } from 'mailspring-exports';
import { RetinaImg } from 'mailspring-component-kit';
import * as OnboardingActions from './onboarding-actions';
import AccountProviders from './account-providers';

const OruMark = () => (
  <svg className="oru-mark" viewBox="0 0 64 64" width="56" height="56" aria-hidden="true">
    <g fill="#E4572E">
      <path d="M6 18 L32 4 L58 18 L58 50 Q58 54 54 54 L10 54 Q6 54 6 50 Z" />
      <path d="M6 18 L32 36 L58 18 L58 24 L32 42 L6 24 Z" fill="#FAF6EF" />
      <path d="M32 36 L58 18 L58 24 L32 42 Z" fill="#C4431F" />
      <path d="M32 4 L32 36 L26 33.4 L26 11.2 Z" fill="#F0845F" />
    </g>
  </svg>
);

export default class AccountChoosePage extends React.Component<{ account?: object }> {
  static displayName = 'AccountChoosePage';

  _renderProviders() {
    return AccountProviders.map(({ icon, displayName, provider }) => (
      <div
        key={provider}
        className={`provider ${provider}`}
        onClick={() => OnboardingActions.chooseAccountProvider(provider)}
      >
        <div className="icon-container">
          <RetinaImg name={icon} mode={RetinaImg.Mode.ContentPreserve} className="icon" />
        </div>
        <span className="provider-name">{displayName}</span>
      </div>
    ));
  }

  render() {
    return (
      <div className="page account-choose">
        <OruMark />
        <h2>{localized('Connect an email account')}</h2>
        <div className="provider-list">{this._renderProviders()}</div>
      </div>
    );
  }
}
