import { localized, React } from 'mailspring-exports';
import * as OnboardingActions from './onboarding-actions';

const OruMark = () => (
  <svg className="oru-mark" viewBox="0 0 64 64" width="72" height="72" aria-hidden="true">
    <g fill="#E4572E">
      <path d="M6 18 L32 4 L58 18 L58 50 Q58 54 54 54 L10 54 Q6 54 6 50 Z" />
      <path d="M6 18 L32 36 L58 18 L58 24 L32 42 L6 24 Z" fill="#FAF6EF" />
      <path d="M32 36 L58 18 L58 24 L32 42 Z" fill="#C4431F" />
      <path d="M32 4 L32 36 L26 33.4 L26 11.2 Z" fill="#F0845F" />
    </g>
  </svg>
);

export default class WelcomePage extends React.Component {
  static displayName = 'WelcomePage';

  _onContinue = () => {
    OnboardingActions.moveToPage('account-choose');
  };

  render() {
    return (
      <div className="page welcome">
        <div className="steps-container">
          <div>
            <OruMark />
            <p className="hero-text" style={{ fontSize: 46, marginTop: 24 }}>
              {localized('Welcome to Oru Mail')}
            </p>
          </div>
        </div>
        <div className="footer">
          <button key="next" className="btn btn-large btn-continue" onClick={this._onContinue}>
            {localized('Get Started')}
          </button>
        </div>
      </div>
    );
  }
}
