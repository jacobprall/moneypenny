import React from 'react'
import SplashNavBar from './splash_nav_bar'
import { Link } from 'react-router-dom'
import ScrollPoint from './scroll_point'
export default function splash_page() {
  return (
    <div className="background">
      <header>
        <SplashNavBar />
      </header>
    
      <main className = "splash-main">
        
        <section className="hero-section">
          <div className="hero-div">
            <img className="hero-image" src={`${window.hero_image}`} alt="hero"/>
            <div className="hero-text">
              <h1>All your money, in one place</h1>
              <h3>When you have control over your finances, life becomes a whole lot simpler.
              <br />This is personal finance like you've never seen it. </h3>
              <Link to="/signup" className="cta-btn">Sign Up Today</Link>
            </div>
          </div>
        </section>
        <ScrollPoint />
          <div id="cards" className="card-container">
            <div className="cards card-1">
              <img className="card-icon" src={`${window.checklist}`} alt=""/>
              <div className="card-header">
                <h1>Your Money. Your Way.</h1>
              </div>
              <div className="card-text">
                <p>moneypenny gives you total control over your personal finances.
    
                </p>
              </div>

            </div>
            <div className="cards card-2">
              <img className="card-icon" src={`${window.house_dollar}`} alt="" />
            <div className="card-header">
              <h1>Budget for the Modern Age</h1>
            </div>
            <div className="card-text">
              <p>Easily create and manage accounts, transactions and goals.</p>
            </div>

            </div>
            <div id="cards" className="cards card-3">
              <img className="card-icon" src={`${window.dial}`} alt="" />
              <div className="card-header">
                <h1>Never miss a bill again</h1>
              </div>
              <div className="card-text">
                <p>Take ownership over your bills with our bill manager.</p>
              </div>

            </div>
          </div>

        <footer>
          <div className="footer-container">
            <div className="footer-links">
              <a href="https://github.com/jacobprall/moneypenny" target="_blank">Github</a>
              <a href="https://www.linkedin.com/in/jacob-prall-01abb867/" target="_blank">LinkedIn</a>
            </div>
          </div>
          <div className="color-bar footer"></div>
        </footer>

      </main>
    </div>
  )
}
