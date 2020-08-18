import React,{ useState } from 'react'
import { Link } from 'react-router-dom'


export default function SessionForm({errors, formType, processForm, processDemoForm, clearErrors}) {

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [pNum, setPNum] = useState("");
  

  const handleSubmit = (e) => {
    e.preventDefault();
    const user = {email, password, p_num: pNum}
    processForm(user);
    
  };

  const handleAlt = () => {
    clearErrors()
  }

  const handleDemo = (e) => {
    const user = {email: 'demo@email.com', password: 'password', p_num: '1234567890'}
    processDemoForm(user)
  }

  const update = (field) => {
    switch (field) {
      case "email":
        return e => setEmail(e.currentTarget.value);
      case "password":
        return e => setPassword(e.currentTarget.value);
      case "pNum":
        return e => setPNum(e.currentTarget.value);
      default:
        return null;
    }
  }
    

  const renderErrors = () => {
    // debugger
    return (
      <ul className="session-errors">
        {errors.map((error, i) => (
          <li className="session-error" key={`error-${i}`}>
            {error}
          </li>
        ))}
      </ul>
    ) 
}

  const formSpecificInputs = () => {
    //Primary form box data
    const formChoice = {};

    // additional info to be added if sign up
    const pNumber = () => (
      <div>
        <label >Phone Number</label> 
        <input type="text" name="p_num" value={pNum} onChange={update('pNum')}/>
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
          onChange={update("email")}
        />
        <br/>
        <label className="password-label">Password</label>
       
        <input className="password-input"
          type="password"
          name="password"
          value={password}
          onChange={update("password")}
        />
        <br/>
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
  

  const formChoice = formSpecificInputs();

  return (
    
    <div className="session-page">
      <div className="alt-nav">
        <p>{formChoice.altTagline}</p>
        <button>
          <Link to={formChoice.altLink} onClick={handleAlt}>{formChoice.altText}</Link>
        </button>
      </div>
      <div className="session-title">
        <div className="session-logo">
          <img className='leaf' src={window.logo} alt="leaf" />
          <h2 className="session-logo">moneypenny</h2> 
        </div>
        <p className="session-tagline">{formChoice.tagline}</p>
      </div>
      <form className="session-form">
        {formChoice.text}
        {formChoice.inputFields()}
        {renderErrors()}
        {formChoice.submit()}
      </form>

      <footer>

      </footer>

    </div>
  )
}
