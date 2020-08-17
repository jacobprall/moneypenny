# == Schema Information
#
# Table name: accounts
#
#  id               :bigint           not null, primary key
#  debit            :boolean          not null
#  account_category :string           not null
#  institution      :string           not null
#  label            :string           not null
#  balance          :float            not null
#  user_id          :integer          not null
#  created_at       :datetime         not null
#  updated_at       :datetime         not null
#
require 'test_helper'

class AccountTest < ActiveSupport::TestCase
  # test "the truth" do
  #   assert true
  # end
end
