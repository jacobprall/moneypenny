import React from 'react'
import { Link } from 'react-router-dom'
// ({ errors, formType, processForm, processDemoForm, clearErrors })
export default class SessionForm extends React.Component {
  constructor(props) {
    super(props);
    this.state = {
      email: "",
      password: "",
      pNum: ""
    }
    this.handleSubmit = this.handleSubmit.bind(this)
    this.handleAlt = this.handleAlt.bind(this)
    this.handleDemo = this.handleDemo.bind(this)
    this.update = this.update.bind(this)
  }



  handleSubmit = (e) => {
    e.preventDefault();
    const user = Object.assign({}, this.state)
    this.props.processForm(user);

  };

  handleAlt = () => {
    this.props.clearErrors()
  }

  handleDemo = (e) => {
    const user = { email: 'demo@email.com', password: 'password' }
    this.props.processDemoForm(user)
  }

  update = (field) => {
    return e => this.setState({
      [field]: e.currentTarget.value
    });
  }


  renderErrors = () => {
    return (
      <ul className="session-errors">
        {this.props.errors.map((error, i) => (
          <li className="session-error" key={`error-${i}`}>
            {error}
          </li>
        ))}
      </ul>
    )
  }

  formChoice = () => {
    //Primary form box data
    const formChoice = {};

    // additional info to be added if sign up
    const pNumber = () => (
      <div>
        <label >Phone Number</label>
        <input type="text" name="p_num" value={pNum} onChange={this.update('pNum')} />
      </div>
    );

    //form specific info
    if (formType === 'login') {
      formChoice.text = "Sign In";
      formChoice.tagline = "All your accounts in one spot.";
      formChoice.pNumber = () => "";
      formChoice.buttonName = "Sign In";
      formChoice.altText = "Sign Up"
      formChoice.altTagline = "Don't have an account yet?"
      formChoice.altLink = '/signup'

    } else {
      formChoice.text = "Create Account";
      formChoice.tagline = "Become a part of the personal finance revolution. Start your moneypenny account today.";
      formChoice.altText = "Sign In"
      formChoice.altTagline = "Already have an account?"
      formChoice.altLink = "/login"
      formChoice.pNumber = pNumber;
    }

    // input fields
    formChoice["inputFields"] = () => (
      <div>
        <label className="email-label">Email Address</label>

        <input className="email-input"
          type="text"
          name="email"
          value={email}
          onChange={this.update("email")}
        />
        <br />
        <label className="password-label">Password</label>

        <input className="password-input"
          type="password"
          name="password"
          value={password}
          onChange={this.update("password")}
        />
        <br />
        {formChoice.pNumber()}

      </div>
    );

    formChoice["submit"] = () => (
      <>
        <button onClick={handleSubmit}>{formChoice.text}</button>
        <button onClick={handleDemo}>Sign in as Demo User</button>
      </>
    )

    return formChoice;
  }


  
  render() {
    return (

      <div className="session-page">
        <div className="alt-nav">
          <p>{this.formChoice().altTagline}</p>
          <button>
            <Link to={this.formChoice().altLink} onClick={handleAlt}>{this.formChoice().altText}</Link>
          </button>
        </div>
        <div className="session-title">
          <div className="session-logo">
            <img className='leaf' src={window.logo} alt="leaf" />

            <Link to="/"> <h2 className="session-logo">moneypenny</h2></Link>
          </div>
          <p className="session-tagline">{this.formChoice().tagline}</p>
        </div>
        <form className="session-form">
          {this.formChoice().text}
          {this.formChoice().inputFields()}
          {this.renderErrors()}
          {this.formChoice().submit()}
        </form>

        <footer>

        </footer>

      </div>
    )
  }
  }
  