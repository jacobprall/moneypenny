# == Schema Information
#
# Table name: users
#
#  id              :bigint           not null, primary key
#  email           :string           not null
#  fname           :string           not null
#  lname           :string           not null
#  password_digest :string           not null
#  session_token   :string           not null
#  created_at      :datetime         not null
#  updated_at      :datetime         not null
#
# Indexes
#
#  index_users_on_email  (email) UNIQUE
#
class User < ApplicationRecord
  validates :fname, :lname, :password_digest, :session_token, presence: true
  validates :email, presence: true, uniqueness: true
  validates :password, length: { minimum: 6, allow_nil: true }


  has_many :accounts, 
  foreign_key: :user_id,
  class_name: :Account

  has_many :bills,
  foreign_key: :user_id,
  class_name: :Bill

  has_many :transactions,
  through: :accounts, 
  source: :transactions 
  
  has_many :goals,
  through: :accounts,
  source: :goals

  attr_reader :password 

  after_initialize :ensure_session_token

  def self.find_by_credentials(em, pw) 
    @user = User.find_by(email: em);
    if @user && @user.is_password?(pw) 
      @user 
    else
      nil
    end
  end

  def password=(pw) 
    self.password_digest = BCrypt::Password.create(pw)
    @password = pw
  end

  def is_password?(pw) 
    BCrypt::Password.new(self.password_digest).is_password?(pw)
  end

  def reset_session_token!
    self.session_token = SecureRandom::urlsafe_base64
    self.save
    self.session_token
  end

  def ensure_session_token
    self.session_token ||= SecureRandom::urlsafe_base64
  end
  
end
